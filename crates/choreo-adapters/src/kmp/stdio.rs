//! A memory kernel spoken to over a child process's pipes.
//!
//! The kernel's embedded edition is a single binary that reads
//! JSON-RPC on stdin and writes it on stdout, one message per line,
//! with its own logs kept on stderr. This starts one, shakes hands,
//! and calls tools on it.
//!
//! # One call at a time
//!
//! Every call takes the same lock, so a second caller waits. That is
//! not a limitation being tolerated, it is the shape of the thing
//! underneath: the kernel's embedded edition takes an exclusive lock
//! on its data directory and serves one writer. Multiplexing calls by
//! request id would need a reader task and a table of waiting callers,
//! and would buy concurrency the other end cannot use.
//!
//! # Only the embedded edition
//!
//! The kernel also runs as a service, and reaching it is a matter of
//! different environment variables on the same binary. That is
//! deliberately not wired here: there is no host asking for it yet,
//! and a configuration surface built for an absent case is one nobody
//! has run.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use super::transport::{KernelAnswer, KernelTransport, KernelTransportError};

/// The version of the tool protocol this client speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

const DEFAULT_BINARY: &str = "rehydration-mcp";
const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Where the kernel is and how patient to be with it.
#[derive(Debug, Clone)]
pub struct StdioKernelTransportConfig {
    binary: String,
    data_dir: PathBuf,
    call_timeout: Duration,
}

impl StdioKernelTransportConfig {
    /// A kernel keeping its memory in `data_dir`.
    ///
    /// The directory is the unit of exclusion: one kernel process per
    /// directory, so two hosts pointed at the same one is a
    /// configuration mistake the kernel will refuse rather than
    /// silently share.
    pub fn new(data_dir: impl Into<PathBuf>) -> Result<Self, DomainError> {
        let data_dir = data_dir.into();
        if data_dir.as_os_str().is_empty() {
            return Err(DomainError::EmptyField {
                field: "kmp.data_dir",
            });
        }
        Ok(Self {
            binary: DEFAULT_BINARY.to_owned(),
            data_dir,
            call_timeout: DEFAULT_CALL_TIMEOUT,
        })
    }

    /// Run a particular binary rather than whatever is on the path.
    #[must_use]
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    /// How long a single tool call may take before the caller is told
    /// the kernel went silent.
    #[must_use]
    pub const fn with_call_timeout(mut self, call_timeout: Duration) -> Self {
        self.call_timeout = call_timeout;
        self
    }

    #[must_use]
    pub fn binary(&self) -> &str {
        &self.binary
    }

    #[must_use]
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    fn environment(&self) -> BTreeMap<&'static str, String> {
        BTreeMap::from([
            ("REHYDRATION_MCP_BACKEND", "embedded".to_owned()),
            (
                "REHYDRATION_MCP_DATA_DIR",
                self.data_dir.display().to_string(),
            ),
        ])
    }
}

/// The pipes to one running kernel.
#[derive(Debug)]
struct Pipes {
    /// Held rather than read: letting this go is what stops the
    /// kernel, so the whole shutdown story is `kill_on_drop` plus
    /// keeping the child alive exactly as long as its pipes.
    _kernel: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

/// A memory kernel running as a child process.
///
/// The kernel dies with the transport that started it, so a host that
/// drops its memory does not leave a process holding a lock on a data
/// directory the next one will want.
#[derive(Debug)]
pub struct StdioKernelTransport {
    pipes: Mutex<Pipes>,
    call_timeout: Duration,
}

impl StdioKernelTransport {
    /// Start a kernel and shake hands with it.
    ///
    /// The handshake is not ceremony: a binary that starts and then
    /// refuses to talk is a failure a caller should hear about now,
    /// not on the first memory it tries to write.
    pub async fn connect(
        config: &StdioKernelTransportConfig,
    ) -> Result<Self, KernelTransportError> {
        let mut command = Command::new(config.binary());
        command
            .envs(config.environment())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|error| KernelTransportError::Unstartable(error.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| KernelTransportError::Unstartable("no stdin pipe".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| KernelTransportError::Unstartable("no stdout pipe".to_owned()))?;
        if let Some(stderr) = child.stderr.take() {
            forward_kernel_logs(stderr);
        }

        let transport = Self {
            pipes: Mutex::new(Pipes {
                _kernel: child,
                stdin,
                stdout: BufReader::new(stdout),
                next_id: 0,
            }),
            call_timeout: config.call_timeout,
        };
        transport.handshake().await?;
        Ok(transport)
    }

    async fn handshake(&self) -> Result<(), KernelTransportError> {
        let mut pipes = self.pipes.lock().await;
        let answer = exchange(
            &mut pipes,
            self.call_timeout,
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "choreographer", "version": env!("CARGO_PKG_VERSION") },
            }),
        )
        .await?;

        if let Some(error) = answer.get("error") {
            return Err(KernelTransportError::Unwelcoming(error.to_string()));
        }

        notify(&mut pipes, "notifications/initialized").await
    }
}

#[async_trait]
impl KernelTransport for StdioKernelTransport {
    async fn call(
        &self,
        tool: &str,
        arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError> {
        let mut pipes = self.pipes.lock().await;
        let answer = exchange(
            &mut pipes,
            self.call_timeout,
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )
        .await?;
        read_answer(&answer)
    }
}

/// Turn one tool response into an answer or a refusal.
///
/// A refusal arrives as ordinary content with a flag set, which is why
/// it is read here and not raised: the words are the kernel's, and the
/// caller above decides what a given refusal means.
fn read_answer(response: &Value) -> Result<KernelAnswer, KernelTransportError> {
    if let Some(error) = response.get("error") {
        return Err(KernelTransportError::Unreadable(error.to_string()));
    }
    let result = response
        .get("result")
        .ok_or_else(|| KernelTransportError::Unreadable("a response with no result".to_owned()))?;

    let text = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Ok(KernelAnswer::Refused(text));
    }

    serde_json::from_str(&text)
        .map(KernelAnswer::Returned)
        .map_err(|error| {
            KernelTransportError::Unreadable(format!("tool content was not a document: {error}"))
        })
}

/// Send a request and wait for the response that answers it.
async fn exchange(
    pipes: &mut Pipes,
    call_timeout: Duration,
    method: &str,
    params: Value,
) -> Result<Value, KernelTransportError> {
    pipes.next_id += 1;
    let id = pipes.next_id;
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });

    write_line(&mut pipes.stdin, &request).await?;

    tokio::time::timeout(call_timeout, read_response(&mut pipes.stdout, id))
        .await
        .map_err(|_| KernelTransportError::Silent {
            seconds: call_timeout.as_secs(),
        })?
}

async fn notify(pipes: &mut Pipes, method: &str) -> Result<(), KernelTransportError> {
    let notification = json!({ "jsonrpc": "2.0", "method": method });
    write_line(&mut pipes.stdin, &notification).await
}

async fn write_line(stdin: &mut ChildStdin, message: &Value) -> Result<(), KernelTransportError> {
    let mut line = serde_json::to_vec(message)
        .map_err(|error| KernelTransportError::Unreadable(error.to_string()))?;
    line.push(b'\n');
    stdin
        .write_all(&line)
        .await
        .map_err(|_| KernelTransportError::Gone)?;
    stdin.flush().await.map_err(|_| KernelTransportError::Gone)
}

/// Read until the response bearing `id` arrives.
///
/// Anything else on the stream — a notification, a response to a
/// request that timed out earlier — is skipped rather than treated as
/// an answer, so one slow call cannot make every later call read the
/// wrong document.
async fn read_response(
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
) -> Result<Value, KernelTransportError> {
    loop {
        let mut line = String::new();
        let read = stdout
            .read_line(&mut line)
            .await
            .map_err(|_| KernelTransportError::Gone)?;
        if read == 0 {
            return Err(KernelTransportError::Gone);
        }
        if line.trim().is_empty() {
            continue;
        }

        let message: Value = serde_json::from_str(line.trim()).map_err(|error| {
            KernelTransportError::Unreadable(format!("a line that was not JSON: {error}"))
        })?;
        match message.get("id").and_then(Value::as_u64) {
            Some(answered) if answered == id => return Ok(message),
            _ => {
                tracing::debug!(
                    expected = id,
                    "skipping a kernel message addressed elsewhere"
                );
            }
        }
    }
}

/// Drain the kernel's own logs into ours.
///
/// Not politeness: an undrained pipe fills, and a kernel blocked on
/// writing a log line stops answering, which would look from here
/// like memory that hangs.
fn forward_kernel_logs(stderr: tokio::process::ChildStderr) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "kmp.kernel", "{line}");
        }
    });
}
