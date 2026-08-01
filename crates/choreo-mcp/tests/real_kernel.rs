//! Real-kernel container integration test for the MCP adapter.
//!
//! Spins up a published choreographer image via `testcontainers`,
//! spawns the locally-built `choreo-mcp` binary in gRPC mode pointing
//! at the container's mapped port, and exercises the JSON-RPC surface:
//!
//!   1. `tools/list` — expect the full 17-tool catalog.
//!   2. `tools/call` — invoke the 4 simplest read-only tools and
//!      assert the response is a well-formed JSON-RPC envelope with
//!      a `result` object.
//!
//! The per-tool 1:1 input/output shape is already covered by the
//! fixture-backed unit tests in `crates/choreo-mcp/src/`. The
//! invariant we want here is the end-to-end wiring:
//!
//!     MCP stdio (binary) ↔ gRPC client ↔ real choreographer
//!
//! Pulls the published `:latest` image instead of building from the
//! repo's `Dockerfile` because `testcontainers` does not expose a
//! "docker build" primitive in its 0.23 series — running the build
//! out-of-band would dilute the contract this test claims.
//!
//! Gated on the `container-tests` feature. Workspace-default
//! `cargo test --workspace` skips this file and stays fast +
//! network-free; the gate runs with
//! `cargo test -p choreo-mcp --features container-tests`.

#![cfg(feature = "container-tests")]

use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

const CHOREO_IMAGE: &str = "ghcr.io/underpass-ai/underpass-choreographer";
const CHOREO_TAG: &str = "latest";
const CHOREO_GRPC_PORT: u16 = 50055;
const CHOREO_HTTP_PORT: u16 = 8080;

/// Newline-delimited JSON-RPC request/response helper.
struct McpStdio {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    line_buf: String,
    next_id: u64,
}

impl McpStdio {
    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&req).expect("serialize request");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .expect("write stdin");
        self.stdin.flush().await.expect("flush stdin");

        self.line_buf.clear();
        let read_fut = self.stdout.read_line(&mut self.line_buf);
        let bytes = timeout(Duration::from_secs(20), read_fut)
            .await
            .expect("MCP response within 20s")
            .expect("read stdout");
        assert!(bytes > 0, "MCP closed stdout before responding");
        serde_json::from_str(self.line_buf.trim_end())
            .unwrap_or_else(|err| panic!("malformed JSON-RPC response: {err}: {}", self.line_buf))
    }
}

fn workspace_target_dir() -> PathBuf {
    // `cargo test` runs the test binary out of
    // `target/<profile>/deps/...`. The MCP binary that this test
    // shells out to lives in the same workspace target, under
    // `target/<profile>/choreo-mcp`. Walking up from the current
    // exe gives us a path that does NOT assume CARGO_MANIFEST_DIR.
    let mut path = env::current_exe().expect("current exe path");
    // pop test binary name + `deps/`
    path.pop();
    path.pop();
    path
}

fn mcp_binary_path() -> PathBuf {
    let mut path = workspace_target_dir();
    path.push("choreo-mcp");
    path
}

async fn spawn_choreographer() -> testcontainers::ContainerAsync<GenericImage> {
    GenericImage::new(CHOREO_IMAGE, CHOREO_TAG)
        .with_exposed_port(CHOREO_GRPC_PORT.tcp())
        .with_exposed_port(CHOREO_HTTP_PORT.tcp())
        // The binary writes the readiness line via tracing JSON. Wait
        // for the gRPC bind log so we don't race the dial-in. Looser
        // pattern keeps the test resilient to tracing format tweaks.
        .with_wait_for(WaitFor::message_on_stderr("servers starting"))
        .with_env_var("CHOREO_GRPC_PORT", CHOREO_GRPC_PORT.to_string())
        .with_env_var("CHOREO_HTTP_PORT", CHOREO_HTTP_PORT.to_string())
        // Disable NATS so the test does not need a broker side-car.
        .with_env_var("CHOREO_NATS_ENABLED", "false")
        // Seed one council so `choreo_list_councils` returns a
        // non-empty list and the test exercises a real read path.
        .with_env_var("CHOREO_SEED_SPECIALTIES", "triage")
        .with_env_var("RUST_LOG", "info")
        .start()
        .await
        .expect("choreographer container should start")
}

fn spawn_mcp(endpoint: &str) -> McpStdio {
    let bin = mcp_binary_path();
    assert!(
        bin.exists(),
        "choreo-mcp binary missing at {}; run `cargo build -p choreo-mcp` first",
        bin.display(),
    );

    let mut child = Command::new(&bin)
        .env("CHOREO_MCP_BACKEND", "grpc")
        .env("CHOREO_MCP_GRPC_ENDPOINT", endpoint)
        .env("RUST_LOG", "choreo_mcp=info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn choreo-mcp");

    let stdin_pipe = child.stdin.take().expect("child stdin");
    let stdout_pipe = child.stdout.take().expect("child stdout");
    let connection = McpStdio {
        stdin: stdin_pipe,
        stdout: BufReader::new(stdout_pipe),
        line_buf: String::new(),
        next_id: 0,
    };

    // Leak the child handle: the test process exits at the end of
    // the test and the OS reaps the spawned MCP binary. Killing it
    // explicitly would race the stdout drain.
    std::mem::forget(child);

    connection
}

#[tokio::test]
async fn mcp_lists_full_tool_catalog_and_calls_read_endpoints() {
    let container = spawn_choreographer().await;
    let port = container
        .get_host_port_ipv4(CHOREO_GRPC_PORT.tcp())
        .await
        .expect("host port");
    let endpoint = format!("http://127.0.0.1:{port}");

    let mut mcp = spawn_mcp(&endpoint);

    // Initialize first — many clients require it, and the MCP
    // server exposes adapter-side metadata in the reply that we
    // log for debug if the next call fails.
    let init = mcp.call("initialize", json!({})).await;
    assert_eq!(
        init.get("jsonrpc").and_then(Value::as_str),
        Some("2.0"),
        "initialize: malformed envelope: {init:?}",
    );
    let init_result = init
        .get("result")
        .unwrap_or_else(|| panic!("initialize had no result: {init:?}"));
    assert!(
        init_result.get("serverInfo").is_some(),
        "initialize missing serverInfo: {init:?}",
    );

    // tools/list — assert the full 1:1 catalog.
    let list = mcp.call("tools/list", json!({})).await;
    let tools = list
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("tools/list missing /result/tools array: {list:?}"));
    assert_eq!(
        tools.len(),
        17,
        "expected 17 tools, got {}: {:?}",
        tools.len(),
        tools.iter().map(|t| t.get("name")).collect::<Vec<_>>()
    );

    // Sanity-check that every tool name starts with `choreo_`.
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("tool entry missing name: {tool:?}"));
        assert!(
            name.starts_with("choreo_"),
            "tool name not prefixed: {name}"
        );
    }

    // tools/call for the 4 simplest read-only RPCs. Each should
    // come back as a well-formed JSON-RPC envelope with a `result`
    // object.
    let simple_tools = [
        "choreo_list_councils",
        "choreo_list_contracts",
        "choreo_get_metrics",
        "choreo_get_status",
    ];

    for tool in simple_tools {
        let resp = mcp
            .call(
                "tools/call",
                json!({
                    "name": tool,
                    "arguments": {},
                }),
            )
            .await;
        assert_eq!(
            resp.get("jsonrpc").and_then(Value::as_str),
            Some("2.0"),
            "{tool}: malformed envelope: {resp:?}",
        );
        let result = resp
            .get("result")
            .unwrap_or_else(|| panic!("{tool}: missing result: {resp:?}"));
        assert!(
            result.is_object(),
            "{tool}: result is not an object: {result:?}",
        );
        // Per MCP spec, tool errors come back as `isError: true`
        // inside `result`, not as a JSON-RPC `error`. A real failure
        // here means the gRPC wiring or the choreographer dropped
        // the call — every read tool should land cleanly.
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        assert!(!is_error, "{tool}: returned isError=true: {result:?}");
    }
}
