//! Runtime-backed [`ExecutorPort`] adapter.
//!
//! This adapter keeps Choreographer domain-agnostic: the core still
//! sees only `ExecutorPort`, while the concrete mapping to
//! `underpass-runtime` gRPC stays here at the edge.

use std::collections::BTreeMap;
use std::path::PathBuf;

use std::time::Duration;

use async_trait::async_trait;
use choreo_core::entities::Proposal;
use choreo_core::error::DomainError;
use choreo_core::ports::{ExecutionOutcome, ExecutorPort};
use choreo_core::value_objects::{Attributes, DurationMs};
use choreo_proto::runtime_v1 as runtime_pb;
use prost_types::{value::Kind as PbKind, ListValue, Struct as PbStruct, Value as PbValue};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use thiserror::Error;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use tracing::{debug, warn};

/// Max time to establish the TCP+TLS connection to the runtime.
const RUNTIME_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Per-RPC deadline applied to every call on the runtime channel. Generous
/// because governed tool invocations can legitimately run for minutes; it
/// is a safety net against an indefinitely-stuck runtime, not a tight SLA.
const RUNTIME_REQUEST_TIMEOUT: Duration = Duration::from_mins(5);

const DEFAULT_RUNTIME_GRPC_ENDPOINT: &str = "http://underpass-runtime:50053";
const DEFAULT_RUNTIME_TENANT_ID: &str = "choreographer";
const DEFAULT_RUNTIME_ACTOR_ID: &str = "choreographer";
const DEFAULT_RUNTIME_ROLE: &str = "developer";

/// TLS-mode override for the Runtime gRPC client.
pub const RUNTIME_TLS_MODE_ENV: &str = "CHOREO_RUNTIME_TLS_MODE";
/// CA bundle the client should trust when verifying the Runtime server.
pub const RUNTIME_TLS_CA_PATH_ENV: &str = "CHOREO_RUNTIME_TLS_CA_PATH";
/// Client certificate PEM (mutual TLS).
pub const RUNTIME_TLS_CERT_PATH_ENV: &str = "CHOREO_RUNTIME_TLS_CERT_PATH";
/// Client private key PEM (mutual TLS).
pub const RUNTIME_TLS_KEY_PATH_ENV: &str = "CHOREO_RUNTIME_TLS_KEY_PATH";
/// Optional TLS SNI/domain override when URL host differs from cert CN/SAN.
pub const RUNTIME_TLS_DOMAIN_NAME_ENV: &str = "CHOREO_RUNTIME_TLS_DOMAIN_NAME";

const KEY_TOOL_NAME: &str = "runtime.tool_name";
const KEY_ARGS: &str = "runtime.args";
const KEY_APPROVED: &str = "runtime.approved";
const KEY_CORRELATION_ID: &str = "runtime.correlation_id";
const KEY_SESSION_ID: &str = "runtime.session_id";
const KEY_SESSION_REQUESTED_ID: &str = "runtime.session.requested_id";
const KEY_SESSION_REPO_URL: &str = "runtime.session.repo_url";
const KEY_SESSION_REPO_REF: &str = "runtime.session.repo_ref";
const KEY_SESSION_SOURCE_REPO_PATH: &str = "runtime.session.source_repo_path";
const KEY_SESSION_ALLOWED_PATHS: &str = "runtime.session.allowed_paths";
const KEY_SESSION_METADATA: &str = "runtime.session.metadata";
const KEY_SESSION_EXPIRES_IN_SECONDS: &str = "runtime.session.expires_in_seconds";

/// Binary-level execution backend selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorBackendConfig {
    Noop,
    Runtime(RuntimeExecutorConfig),
}

impl ExecutorBackendConfig {
    /// Load executor selection from `CHOREO_*` environment variables.
    ///
    /// Supported variables:
    ///
    /// - `CHOREO_EXECUTOR_KIND`: `noop` (default) or `runtime`
    /// - `CHOREO_RUNTIME_GRPC_ENDPOINT`
    /// - `CHOREO_RUNTIME_PRINCIPAL_TENANT_ID`
    /// - `CHOREO_RUNTIME_PRINCIPAL_ACTOR_ID`
    /// - `CHOREO_RUNTIME_PRINCIPAL_ROLES` (comma-separated)
    pub fn from_env() -> Result<Self, DomainError> {
        let kind = std::env::var("CHOREO_EXECUTOR_KIND")
            .unwrap_or_else(|_| "noop".to_owned())
            .trim()
            .to_ascii_lowercase();

        match kind.as_str() {
            "" | "noop" => Ok(Self::Noop),
            "runtime" => Ok(Self::Runtime(RuntimeExecutorConfig::from_env()?)),
            _ => Err(DomainError::InvariantViolated {
                reason: "invalid executor backend kind",
            }),
        }
    }
}

/// Static technical identity used when the adapter creates ephemeral
/// Runtime sessions on behalf of Choreographer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePrincipal {
    pub tenant_id: String,
    pub actor_id: String,
    pub roles: Vec<String>,
}

impl RuntimePrincipal {
    fn to_proto(&self) -> runtime_pb::Principal {
        runtime_pb::Principal {
            tenant_id: self.tenant_id.clone(),
            actor_id: self.actor_id.clone(),
            roles: self.roles.clone(),
        }
    }
}

/// Configuration for the Runtime executor adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutorConfig {
    pub grpc_endpoint: String,
    pub principal: RuntimePrincipal,
    pub tls: RuntimeClientTlsConfig,
}

impl RuntimeExecutorConfig {
    pub fn from_env() -> Result<Self, DomainError> {
        let grpc_endpoint = env_or_default(
            "CHOREO_RUNTIME_GRPC_ENDPOINT",
            DEFAULT_RUNTIME_GRPC_ENDPOINT,
        )?;
        let tenant_id = env_or_default(
            "CHOREO_RUNTIME_PRINCIPAL_TENANT_ID",
            DEFAULT_RUNTIME_TENANT_ID,
        )?;
        let actor_id = env_or_default(
            "CHOREO_RUNTIME_PRINCIPAL_ACTOR_ID",
            DEFAULT_RUNTIME_ACTOR_ID,
        )?;
        let roles = parse_roles(std::env::var("CHOREO_RUNTIME_PRINCIPAL_ROLES").ok())?;
        let tls = RuntimeClientTlsConfig::from_env_for_endpoint(Some(&grpc_endpoint))?;

        Ok(Self {
            grpc_endpoint,
            principal: RuntimePrincipal {
                tenant_id,
                actor_id,
                roles,
            },
            tls,
        })
    }
}

/// TLS posture for the outbound gRPC client to underpass-runtime.
///
/// Mirrors the shape used by the MCP adapter: `Disabled` (plain
/// HTTP/2), `Server` (one-way TLS), or `Mutual` (mTLS). Auto-detected
/// from environment but every variant is also constructable directly
/// so tests stay independent of process env.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeClientTlsConfig {
    pub mode: RuntimeClientTlsMode,
    pub ca_path: Option<PathBuf>,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub domain_name: Option<String>,
}

/// Operator-visible TLS posture options for the Runtime client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeClientTlsMode {
    Disabled,
    Server,
    Mutual,
}

impl RuntimeClientTlsMode {
    /// Stable label for logs/metadata.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Server => "server",
            Self::Mutual => "mutual",
        }
    }
}

impl Default for RuntimeClientTlsConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

impl RuntimeClientTlsConfig {
    /// Explicitly disabled TLS — plain HTTP/2 over TCP.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            mode: RuntimeClientTlsMode::Disabled,
            ca_path: None,
            cert_path: None,
            key_path: None,
            domain_name: None,
        }
    }

    /// One-way TLS with a caller-supplied CA bundle.
    #[must_use]
    pub fn server(ca_path: impl Into<PathBuf>, domain_name: Option<String>) -> Self {
        Self {
            mode: RuntimeClientTlsMode::Server,
            ca_path: Some(ca_path.into()),
            cert_path: None,
            key_path: None,
            domain_name,
        }
    }

    /// Mutual TLS: client presents identity alongside verifying server.
    #[must_use]
    pub fn mutual(
        ca_path: impl Into<PathBuf>,
        cert_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        domain_name: Option<String>,
    ) -> Self {
        Self {
            mode: RuntimeClientTlsMode::Mutual,
            ca_path: Some(ca_path.into()),
            cert_path: Some(cert_path.into()),
            key_path: Some(key_path.into()),
            domain_name,
        }
    }

    /// Resolve the TLS posture from env with the same auto-detection
    /// the MCP adapter uses: `https://` endpoint OR `_CA_PATH` set OR
    /// `_DOMAIN_NAME` set → `Server`; presence of `_CERT_PATH` /
    /// `_KEY_PATH` → `Mutual`; explicit `_TLS_MODE` always wins.
    pub fn from_env_for_endpoint(endpoint: Option<&str>) -> Result<Self, DomainError> {
        let ca_path = optional_env_path(RUNTIME_TLS_CA_PATH_ENV);
        let cert_path = optional_env_path(RUNTIME_TLS_CERT_PATH_ENV);
        let key_path = optional_env_path(RUNTIME_TLS_KEY_PATH_ENV);
        let domain_name = optional_env_string(RUNTIME_TLS_DOMAIN_NAME_ENV);
        let server_tls_requested = ca_path.is_some()
            || domain_name.is_some()
            || endpoint.is_some_and(|endpoint| endpoint.trim().starts_with("https://"));

        let explicit_mode = optional_env_string(RUNTIME_TLS_MODE_ENV);
        let mode = match explicit_mode.as_deref() {
            Some(value) => parse_runtime_tls_mode(value).ok_or(DomainError::InvariantViolated {
                reason: "CHOREO_RUNTIME_TLS_MODE must be one of: disabled, server, mutual",
            })?,
            None => {
                if cert_path.is_some() || key_path.is_some() {
                    RuntimeClientTlsMode::Mutual
                } else if server_tls_requested {
                    RuntimeClientTlsMode::Server
                } else {
                    RuntimeClientTlsMode::Disabled
                }
            }
        };

        if mode == RuntimeClientTlsMode::Mutual && (cert_path.is_none() || key_path.is_none()) {
            return Err(DomainError::InvariantViolated {
                reason: "CHOREO_RUNTIME_TLS_MODE=mutual requires both _CERT_PATH and _KEY_PATH",
            });
        }

        Ok(Self {
            mode,
            ca_path,
            cert_path,
            key_path,
            domain_name,
        })
    }

    /// Stable label for startup logs and `Debug`-style output.
    #[must_use]
    pub fn mode_name(&self) -> &'static str {
        self.mode.as_str()
    }
}

fn parse_runtime_tls_mode(value: &str) -> Option<RuntimeClientTlsMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" | "disable" | "off" | "false" | "none" => Some(RuntimeClientTlsMode::Disabled),
        "server" | "tls" => Some(RuntimeClientTlsMode::Server),
        "mutual" | "mtls" | "m-tls" => Some(RuntimeClientTlsMode::Mutual),
        _ => None,
    }
}

fn optional_env_path(name: &str) -> Option<PathBuf> {
    optional_env_string(name).map(PathBuf::from)
}

fn optional_env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// When TLS is enabled, rewrite an `http://` endpoint to `https://`
/// so callers can flip the TLS knob with a single env change.
fn endpoint_uri_for_tls_mode(endpoint: &str, mode: RuntimeClientTlsMode) -> String {
    if mode == RuntimeClientTlsMode::Disabled {
        return endpoint.to_string();
    }
    endpoint.strip_prefix("http://").map_or_else(
        || endpoint.to_string(),
        |without_scheme| format!("https://{without_scheme}"),
    )
}

#[derive(Debug, Error)]
pub enum RuntimeExecutorConnectError {
    #[error("invalid runtime gRPC endpoint")]
    InvalidEndpoint,

    #[error("runtime gRPC connection failed: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("failed to read runtime TLS material at {path}: {source}")]
    TlsReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Runtime-backed executor.
#[derive(Debug, Clone)]
pub struct RuntimeExecutor {
    channel: Channel,
    config: RuntimeExecutorConfig,
}

impl RuntimeExecutor {
    pub async fn connect(
        config: RuntimeExecutorConfig,
    ) -> Result<Self, RuntimeExecutorConnectError> {
        let endpoint_uri = endpoint_uri_for_tls_mode(&config.grpc_endpoint, config.tls.mode);
        // Bound both connection establishment and every RPC on this channel
        // so a stuck runtime can never hang a deliberation indefinitely.
        // The request bound is generous (long-running governed tools are
        // legitimate); the connect bound is tight.
        let mut endpoint = Endpoint::from_shared(endpoint_uri)
            .map_err(|_| RuntimeExecutorConnectError::InvalidEndpoint)?
            .connect_timeout(RUNTIME_CONNECT_TIMEOUT)
            .timeout(RUNTIME_REQUEST_TIMEOUT);

        if config.tls.mode != RuntimeClientTlsMode::Disabled {
            let tls_config = build_runtime_client_tls(&config.tls).await?;
            endpoint = endpoint.tls_config(tls_config)?;
        }

        let channel = endpoint.connect().await?;
        debug!(
            grpc_endpoint = config.grpc_endpoint.as_str(),
            tls_mode = config.tls.mode_name(),
            "runtime executor connected"
        );
        Ok(Self { channel, config })
    }
}

async fn build_runtime_client_tls(
    tls: &RuntimeClientTlsConfig,
) -> Result<ClientTlsConfig, RuntimeExecutorConnectError> {
    let mut config = ClientTlsConfig::new();

    if let Some(domain) = tls.domain_name.as_deref() {
        config = config.domain_name(domain.to_string());
    }

    if let Some(ca_path) = tls.ca_path.as_ref() {
        let pem = tokio::fs::read(ca_path).await.map_err(|source| {
            RuntimeExecutorConnectError::TlsReadFailed {
                path: ca_path.display().to_string(),
                source,
            }
        })?;
        config = config.ca_certificate(Certificate::from_pem(pem));
    }

    if tls.mode == RuntimeClientTlsMode::Mutual {
        // `from_env_for_endpoint` already enforced that both paths are set;
        // direct constructors land here too because `mutual(...)` requires them.
        let cert_path = tls
            .cert_path
            .as_ref()
            .expect("mutual TLS without cert_path");
        let key_path = tls.key_path.as_ref().expect("mutual TLS without key_path");
        let cert = tokio::fs::read(cert_path).await.map_err(|source| {
            RuntimeExecutorConnectError::TlsReadFailed {
                path: cert_path.display().to_string(),
                source,
            }
        })?;
        let key = tokio::fs::read(key_path).await.map_err(|source| {
            RuntimeExecutorConnectError::TlsReadFailed {
                path: key_path.display().to_string(),
                source,
            }
        })?;
        config = config.identity(Identity::from_pem(cert, key));
    }

    Ok(config)
}

#[async_trait]
impl ExecutorPort for RuntimeExecutor {
    async fn execute(
        &self,
        winner: &Proposal,
        options: &Attributes,
    ) -> Result<ExecutionOutcome, DomainError> {
        let request =
            RuntimeExecutionRequest::from_inputs(winner, options, &self.config.principal)?;
        let mut session_client =
            runtime_pb::session_service_client::SessionServiceClient::new(self.channel.clone());
        let mut invocation_client =
            runtime_pb::invocation_service_client::InvocationServiceClient::new(
                self.channel.clone(),
            );

        let (session_id, created_session_here) = if let Some(existing) = request.session_id.clone()
        {
            (existing, false)
        } else {
            let response = session_client
                .create_session(request.create_session_request())
                .await
                .map_err(|status| map_runtime_status(&status))?;
            let session = response
                .into_inner()
                .session
                .ok_or(DomainError::InvariantViolated {
                    reason: "runtime create_session returned no session",
                })?;
            (session.id, true)
        };

        let invoke_result = invocation_client
            .invoke_tool(request.invoke_tool_request(session_id.clone()))
            .await;

        if created_session_here {
            best_effort_close_session(session_id.clone(), session_client).await;
        }

        let invocation = invoke_result
            .map_err(|status| map_runtime_status(&status))?
            .into_inner()
            .invocation
            .ok_or(DomainError::InvariantViolated {
                reason: "runtime invoke_tool returned no invocation",
            })?;

        debug!(
            tool_name = invocation.tool_name.as_str(),
            invocation_id = invocation.id.as_str(),
            status = invocation_status_name(invocation.status),
            "runtime executor received terminal invocation"
        );

        execution_outcome_from_invocation(invocation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeExecutionRequest {
    session_id: Option<String>,
    requested_session_id: Option<String>,
    repo_url: Option<String>,
    repo_ref: Option<String>,
    source_repo_path: Option<String>,
    allowed_paths: Vec<String>,
    session_metadata: BTreeMap<String, String>,
    expires_in_seconds: Option<u32>,
    principal: RuntimePrincipal,
    tool_name: String,
    correlation_id: String,
    approved: bool,
    args: JsonMap<String, JsonValue>,
}

impl RuntimeExecutionRequest {
    fn from_inputs(
        winner: &Proposal,
        options: &Attributes,
        default_principal: &RuntimePrincipal,
    ) -> Result<Self, DomainError> {
        let tool_name = required_string_value(options, winner, KEY_TOOL_NAME)?;
        let correlation_id = optional_string_value(options, winner, KEY_CORRELATION_ID)?
            .unwrap_or_else(|| winner.id().as_str().to_owned());
        let approved = optional_bool_value(options, winner, KEY_APPROVED)?.unwrap_or(false);
        let session_id = optional_string_value(options, winner, KEY_SESSION_ID)?;
        let requested_session_id =
            optional_string_value(options, winner, KEY_SESSION_REQUESTED_ID)?;
        let repo_url = optional_string_value(options, winner, KEY_SESSION_REPO_URL)?;
        let repo_ref = optional_string_value(options, winner, KEY_SESSION_REPO_REF)?;
        let source_repo_path =
            optional_string_value(options, winner, KEY_SESSION_SOURCE_REPO_PATH)?;
        let allowed_paths =
            optional_string_array(options, winner, KEY_SESSION_ALLOWED_PATHS)?.unwrap_or_default();
        let expires_in_seconds =
            optional_u32_value(options, winner, KEY_SESSION_EXPIRES_IN_SECONDS)?;
        let args = optional_object_value(options, winner, KEY_ARGS)?.unwrap_or_default();

        let mut session_metadata =
            optional_string_map(options, winner, KEY_SESSION_METADATA)?.unwrap_or_default();
        session_metadata
            .entry("choreographer.proposal_id".to_owned())
            .or_insert_with(|| winner.id().as_str().to_owned());
        session_metadata
            .entry("choreographer.author_id".to_owned())
            .or_insert_with(|| winner.author().as_str().to_owned());
        session_metadata
            .entry("choreographer.specialty".to_owned())
            .or_insert_with(|| winner.specialty().as_str().to_owned());

        Ok(Self {
            session_id,
            requested_session_id,
            repo_url,
            repo_ref,
            source_repo_path,
            allowed_paths,
            session_metadata,
            expires_in_seconds,
            principal: default_principal.clone(),
            tool_name,
            correlation_id,
            approved,
            args,
        })
    }

    fn create_session_request(&self) -> runtime_pb::CreateSessionRequest {
        runtime_pb::CreateSessionRequest {
            session_id: self.requested_session_id.clone().unwrap_or_default(),
            repo_url: self.repo_url.clone().unwrap_or_default(),
            repo_ref: self.repo_ref.clone().unwrap_or_default(),
            source_repo_path: self.source_repo_path.clone().unwrap_or_default(),
            allowed_paths: self.allowed_paths.clone(),
            principal: Some(self.principal.to_proto()),
            metadata: self.session_metadata.clone().into_iter().collect(),
            expires_in_seconds: self
                .expires_in_seconds
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_default(),
        }
    }

    fn invoke_tool_request(&self, session_id: String) -> runtime_pb::InvokeToolRequest {
        runtime_pb::InvokeToolRequest {
            session_id,
            tool_name: self.tool_name.clone(),
            correlation_id: self.correlation_id.clone(),
            args: Some(PbStruct {
                fields: self
                    .args
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_pb_value(v)))
                    .collect(),
            }),
            approved: self.approved,
        }
    }
}

async fn best_effort_close_session(
    session_id: String,
    mut client: runtime_pb::session_service_client::SessionServiceClient<Channel>,
) {
    if let Err(err) = client
        .close_session(runtime_pb::CloseSessionRequest { session_id })
        .await
    {
        warn!(error = %err, "runtime executor failed to close ephemeral session");
    }
}

fn execution_outcome_from_invocation(
    invocation: runtime_pb::Invocation,
) -> Result<ExecutionOutcome, DomainError> {
    let status = runtime_pb::InvocationStatus::try_from(invocation.status)
        .unwrap_or(runtime_pb::InvocationStatus::Unspecified);
    if matches!(
        status,
        runtime_pb::InvocationStatus::Unspecified | runtime_pb::InvocationStatus::Running
    ) {
        return Err(DomainError::InvariantViolated {
            reason: "runtime invocation did not reach terminal state",
        });
    }

    let mut output = BTreeMap::new();
    output.insert(
        "runtime.status".to_owned(),
        JsonValue::String(invocation_status_name(status).to_owned()),
    );
    output.insert(
        "runtime.session_id".to_owned(),
        JsonValue::String(invocation.session_id.clone()),
    );
    output.insert(
        "runtime.tool_name".to_owned(),
        JsonValue::String(invocation.tool_name.clone()),
    );
    output.insert(
        "runtime.correlation_id".to_owned(),
        JsonValue::String(invocation.correlation_id.clone()),
    );
    output.insert(
        "runtime.exit_code".to_owned(),
        JsonValue::Number(JsonNumber::from(invocation.exit_code)),
    );

    if let Some(value) = invocation.output {
        output.insert("runtime.output".to_owned(), pb_value_to_json(value));
    }
    if !invocation.output_ref.is_empty() {
        output.insert(
            "runtime.output_ref".to_owned(),
            JsonValue::String(invocation.output_ref),
        );
    }
    if !invocation.logs_ref.is_empty() {
        output.insert(
            "runtime.logs_ref".to_owned(),
            JsonValue::String(invocation.logs_ref),
        );
    }
    if let Some(error) = invocation.error {
        let mut error_obj = JsonMap::new();
        if !error.code.is_empty() {
            error_obj.insert("code".to_owned(), JsonValue::String(error.code));
        }
        if !error.message.is_empty() {
            error_obj.insert("message".to_owned(), JsonValue::String(error.message));
        }
        output.insert("runtime.error".to_owned(), JsonValue::Object(error_obj));
    }
    if !invocation.artifacts.is_empty() {
        let artifacts = invocation
            .artifacts
            .into_iter()
            .map(|artifact| {
                let mut obj = JsonMap::new();
                obj.insert("id".to_owned(), JsonValue::String(artifact.id));
                obj.insert("name".to_owned(), JsonValue::String(artifact.name));
                obj.insert("path".to_owned(), JsonValue::String(artifact.path));
                obj.insert(
                    "content_type".to_owned(),
                    JsonValue::String(artifact.content_type),
                );
                obj.insert(
                    "size_bytes".to_owned(),
                    JsonValue::Number(JsonNumber::from(artifact.size_bytes)),
                );
                obj.insert("sha256".to_owned(), JsonValue::String(artifact.sha256));
                JsonValue::Object(obj)
            })
            .collect();
        output.insert("runtime.artifacts".to_owned(), JsonValue::Array(artifacts));
    }

    Ok(ExecutionOutcome {
        execution_id: invocation.id,
        succeeded: matches!(status, runtime_pb::InvocationStatus::Succeeded),
        duration: DurationMs::try_from(invocation.duration_ms)?,
        output: Attributes::new(output)?,
    })
}

fn map_runtime_status(status: &tonic::Status) -> DomainError {
    match status.code() {
        tonic::Code::NotFound => DomainError::NotFound {
            what: "runtime resource",
        },
        tonic::Code::InvalidArgument | tonic::Code::FailedPrecondition => {
            DomainError::InvariantViolated {
                reason: "runtime rejected request",
            }
        }
        _ => DomainError::InvariantViolated {
            reason: "runtime transport error",
        },
    }
}

fn invocation_status_name(status: impl Into<i32>) -> &'static str {
    match runtime_pb::InvocationStatus::try_from(status.into())
        .unwrap_or(runtime_pb::InvocationStatus::Unspecified)
    {
        runtime_pb::InvocationStatus::Succeeded => "succeeded",
        runtime_pb::InvocationStatus::Failed => "failed",
        runtime_pb::InvocationStatus::Denied => "denied",
        runtime_pb::InvocationStatus::Running => "running",
        runtime_pb::InvocationStatus::Unspecified => "unspecified",
    }
}

fn env_or_default(key: &str, default: &str) -> Result<String, DomainError> {
    let value = std::env::var(key).unwrap_or_else(|_| default.to_owned());
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyField {
            field: "runtime.config",
        });
    }
    Ok(trimmed.to_owned())
}

fn parse_roles(raw: Option<String>) -> Result<Vec<String>, DomainError> {
    let roles = raw
        .unwrap_or_else(|| DEFAULT_RUNTIME_ROLE.to_owned())
        .split(',')
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if roles.is_empty() {
        return Err(DomainError::EmptyCollection {
            field: "runtime.principal.roles",
        });
    }
    Ok(roles)
}

fn required_string_value(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<String, DomainError> {
    optional_string_value(options, winner, key)?.ok_or(DomainError::EmptyField { field: key })
}

fn optional_string_value(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<Option<String>, DomainError> {
    match lookup_value(options, winner, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(value)) if value.trim().is_empty() => Ok(None),
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(DomainError::InvariantViolated {
            reason: "runtime string option must be a string",
        }),
    }
}

fn optional_bool_value(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<Option<bool>, DomainError> {
    match lookup_value(options, winner, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(DomainError::InvariantViolated {
            reason: "runtime bool option must be a bool",
        }),
    }
}

fn optional_u32_value(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<Option<u32>, DomainError> {
    match lookup_value(options, winner, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Number(value)) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or(DomainError::InvariantViolated {
                reason: "runtime numeric option must be a non-negative integer",
            }),
        Some(_) => Err(DomainError::InvariantViolated {
            reason: "runtime numeric option must be a non-negative integer",
        }),
    }
}

fn optional_string_array(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<Option<Vec<String>>, DomainError> {
    match lookup_value(options, winner, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Array(values)) => values
            .iter()
            .map(|value| match value {
                JsonValue::String(item) if !item.trim().is_empty() => Ok(item.clone()),
                _ => Err(DomainError::InvariantViolated {
                    reason: "runtime string array option must be an array of strings",
                }),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(DomainError::InvariantViolated {
            reason: "runtime string array option must be an array of strings",
        }),
    }
}

fn optional_string_map(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<Option<BTreeMap<String, String>>, DomainError> {
    match lookup_value(options, winner, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Object(values)) => values
            .iter()
            .map(|(field, value)| match value {
                JsonValue::String(text) => Ok((field.clone(), text.clone())),
                _ => Err(DomainError::InvariantViolated {
                    reason: "runtime metadata must be an object of strings",
                }),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Some),
        Some(_) => Err(DomainError::InvariantViolated {
            reason: "runtime metadata must be an object of strings",
        }),
    }
}

fn optional_object_value(
    options: &Attributes,
    winner: &Proposal,
    key: &'static str,
) -> Result<Option<JsonMap<String, JsonValue>>, DomainError> {
    match lookup_value(options, winner, key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Object(value)) => Ok(Some(value.clone())),
        Some(_) => Err(DomainError::InvariantViolated {
            reason: "runtime args must be a JSON object",
        }),
    }
}

fn lookup_value<'a>(
    options: &'a Attributes,
    winner: &'a Proposal,
    key: &str,
) -> Option<&'a JsonValue> {
    options.get(key).or_else(|| winner.attributes().get(key))
}

fn json_to_pb_value(value: &JsonValue) -> PbValue {
    let kind = match value {
        JsonValue::Null => PbKind::NullValue(0),
        JsonValue::Bool(boolean) => PbKind::BoolValue(*boolean),
        JsonValue::Number(number) => PbKind::NumberValue(number.as_f64().unwrap_or_default()),
        JsonValue::String(text) => PbKind::StringValue(text.clone()),
        JsonValue::Array(values) => PbKind::ListValue(ListValue {
            values: values.iter().map(json_to_pb_value).collect(),
        }),
        JsonValue::Object(object) => PbKind::StructValue(PbStruct {
            fields: object
                .iter()
                .map(|(key, value)| (key.clone(), json_to_pb_value(value)))
                .collect(),
        }),
    };
    PbValue { kind: Some(kind) }
}

fn pb_value_to_json(value: PbValue) -> JsonValue {
    match value.kind {
        None | Some(PbKind::NullValue(_)) => JsonValue::Null,
        Some(PbKind::BoolValue(boolean)) => JsonValue::Bool(boolean),
        Some(PbKind::NumberValue(number)) => {
            JsonNumber::from_f64(number).map_or(JsonValue::Null, JsonValue::Number)
        }
        Some(PbKind::StringValue(text)) => JsonValue::String(text),
        Some(PbKind::ListValue(list)) => {
            JsonValue::Array(list.values.into_iter().map(pb_value_to_json).collect())
        }
        Some(PbKind::StructValue(struct_value)) => JsonValue::Object(
            struct_value
                .fields
                .into_iter()
                .map(|(key, value)| (key, pb_value_to_json(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use choreo_core::value_objects::{AgentId, ProposalId, Specialty};
    use serde_json::json;
    use time::macros::datetime;
    use tokio::sync::{oneshot, Mutex};
    use tonic::{transport::Server, Request, Response, Status};

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[derive(Debug, Clone)]
    struct RecordingRuntime {
        create_requests: Arc<Mutex<Vec<runtime_pb::CreateSessionRequest>>>,
        invoke_requests: Arc<Mutex<Vec<runtime_pb::InvokeToolRequest>>>,
        close_requests: Arc<Mutex<Vec<runtime_pb::CloseSessionRequest>>>,
        invocation_status: runtime_pb::InvocationStatus,
        invoke_error: Option<tonic::Code>,
    }

    impl RecordingRuntime {
        fn new(status: runtime_pb::InvocationStatus) -> Self {
            Self {
                create_requests: Arc::new(Mutex::new(Vec::new())),
                invoke_requests: Arc::new(Mutex::new(Vec::new())),
                close_requests: Arc::new(Mutex::new(Vec::new())),
                invocation_status: status,
                invoke_error: None,
            }
        }

        fn with_invoke_error(code: tonic::Code) -> Self {
            let mut runtime = Self::new(runtime_pb::InvocationStatus::Succeeded);
            runtime.invoke_error = Some(code);
            runtime
        }
    }

    #[async_trait]
    impl runtime_pb::session_service_server::SessionService for RecordingRuntime {
        async fn create_session(
            &self,
            request: Request<runtime_pb::CreateSessionRequest>,
        ) -> Result<Response<runtime_pb::CreateSessionResponse>, Status> {
            let request = request.into_inner();
            self.create_requests.lock().await.push(request.clone());
            Ok(Response::new(runtime_pb::CreateSessionResponse {
                session: Some(runtime_pb::Session {
                    id: "sess-123".to_owned(),
                    workspace_path: "/tmp/workspace".to_owned(),
                    runtime: None,
                    repo_url: request.repo_url,
                    repo_ref: request.repo_ref,
                    allowed_paths: request.allowed_paths,
                    principal: request.principal,
                    metadata: request.metadata,
                    created_at: None,
                    expires_at: None,
                }),
            }))
        }

        async fn close_session(
            &self,
            request: Request<runtime_pb::CloseSessionRequest>,
        ) -> Result<Response<runtime_pb::CloseSessionResponse>, Status> {
            let request = request.into_inner();
            self.close_requests.lock().await.push(request);
            Ok(Response::new(runtime_pb::CloseSessionResponse {
                closed: true,
            }))
        }
    }

    #[async_trait]
    impl runtime_pb::invocation_service_server::InvocationService for RecordingRuntime {
        async fn invoke_tool(
            &self,
            request: Request<runtime_pb::InvokeToolRequest>,
        ) -> Result<Response<runtime_pb::InvokeToolResponse>, Status> {
            if let Some(code) = self.invoke_error {
                return Err(Status::new(code, "boom"));
            }
            let request = request.into_inner();
            self.invoke_requests.lock().await.push(request.clone());
            Ok(Response::new(runtime_pb::InvokeToolResponse {
                invocation: Some(runtime_pb::Invocation {
                    id: "inv-456".to_owned(),
                    session_id: request.session_id,
                    tool_name: request.tool_name,
                    correlation_id: request.correlation_id,
                    status: self.invocation_status as i32,
                    started_at: None,
                    completed_at: None,
                    duration_ms: 321,
                    trace_name: String::new(),
                    span_name: String::new(),
                    exit_code: 17,
                    output: Some(json_to_pb_value(&json!({"ok": true}))),
                    output_ref: "artifact://output".to_owned(),
                    logs: Vec::new(),
                    logs_ref: "artifact://logs".to_owned(),
                    artifacts: vec![runtime_pb::Artifact {
                        id: "art-1".to_owned(),
                        name: "report.json".to_owned(),
                        path: "/tmp/report.json".to_owned(),
                        content_type: "application/json".to_owned(),
                        size_bytes: 12,
                        sha256: "abc".to_owned(),
                        created_at: None,
                    }],
                    error: Some(runtime_pb::Error {
                        code: "policy_denied".to_owned(),
                        message: "approval required".to_owned(),
                    }),
                }),
            }))
        }
    }

    async fn spawn_runtime(
        runtime: RecordingRuntime,
    ) -> (SocketAddr, oneshot::Sender<()>, RecordingRuntime) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
        let session = runtime.clone();
        let invocation = runtime.clone();

        tokio::spawn(async move {
            Server::builder()
                .add_service(runtime_pb::session_service_server::SessionServiceServer::new(session))
                .add_service(
                    runtime_pb::invocation_service_server::InvocationServiceServer::new(invocation),
                )
                .serve_with_incoming_shutdown(incoming, async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        (addr, shutdown_tx, runtime)
    }

    fn proposal_with_runtime_attributes() -> Proposal {
        let attrs = Attributes::new(BTreeMap::from([
            (KEY_TOOL_NAME.to_owned(), json!("fs.write_file")),
            (
                KEY_ARGS.to_owned(),
                json!({"path": "reports/out.txt", "content": "hello"}),
            ),
            (
                KEY_SESSION_METADATA.to_owned(),
                json!({"runner_profile": "safe", "governance_profile": "strict"}),
            ),
            (
                KEY_SESSION_ALLOWED_PATHS.to_owned(),
                json!(["reports", "scratch"]),
            ),
        ]))
        .unwrap();

        Proposal::new(
            ProposalId::new("p1").unwrap(),
            AgentId::new("agent-1").unwrap(),
            Specialty::new("triage").unwrap(),
            "write a report",
            attrs,
            datetime!(2026-04-25 12:00:00 UTC),
        )
        .unwrap()
    }

    fn runtime_config(endpoint: String) -> RuntimeExecutorConfig {
        RuntimeExecutorConfig {
            grpc_endpoint: endpoint,
            principal: RuntimePrincipal {
                tenant_id: "tenant-a".to_owned(),
                actor_id: "choreographer".to_owned(),
                roles: vec!["developer".to_owned()],
            },
            tls: RuntimeClientTlsConfig::disabled(),
        }
    }

    fn clear_runtime_env() {
        for (key, _) in std::env::vars() {
            if key == "CHOREO_EXECUTOR_KIND"
                || key == "CHOREO_RUNTIME_GRPC_ENDPOINT"
                || key == "CHOREO_RUNTIME_PRINCIPAL_TENANT_ID"
                || key == "CHOREO_RUNTIME_PRINCIPAL_ACTOR_ID"
                || key == "CHOREO_RUNTIME_PRINCIPAL_ROLES"
                || key == RUNTIME_TLS_MODE_ENV
                || key == RUNTIME_TLS_CA_PATH_ENV
                || key == RUNTIME_TLS_CERT_PATH_ENV
                || key == RUNTIME_TLS_KEY_PATH_ENV
                || key == RUNTIME_TLS_DOMAIN_NAME_ENV
            {
                std::env::remove_var(key);
            }
        }
    }

    #[tokio::test]
    async fn defaults_to_noop_backend() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        std::env::remove_var("CHOREO_EXECUTOR_KIND");
        let cfg = ExecutorBackendConfig::from_env().unwrap();
        assert!(matches!(cfg, ExecutorBackendConfig::Noop));
    }

    #[tokio::test]
    async fn runtime_backend_loads_from_env() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        std::env::set_var("CHOREO_EXECUTOR_KIND", "runtime");
        std::env::set_var(
            "CHOREO_RUNTIME_GRPC_ENDPOINT",
            "http://runtime.example:50053",
        );
        std::env::set_var("CHOREO_RUNTIME_PRINCIPAL_TENANT_ID", "tenant-x");
        std::env::set_var("CHOREO_RUNTIME_PRINCIPAL_ACTOR_ID", "actor-y");
        std::env::set_var("CHOREO_RUNTIME_PRINCIPAL_ROLES", "developer,ops");

        let cfg = ExecutorBackendConfig::from_env().unwrap();
        assert_eq!(
            cfg,
            ExecutorBackendConfig::Runtime(RuntimeExecutorConfig {
                grpc_endpoint: "http://runtime.example:50053".to_owned(),
                principal: RuntimePrincipal {
                    tenant_id: "tenant-x".to_owned(),
                    actor_id: "actor-y".to_owned(),
                    roles: vec!["developer".to_owned(), "ops".to_owned()],
                },
                tls: RuntimeClientTlsConfig::disabled(),
            })
        );

        clear_runtime_env();
    }

    #[tokio::test]
    async fn execute_creates_session_invokes_tool_and_closes_ephemeral_session() {
        let (addr, shutdown, runtime) = spawn_runtime(RecordingRuntime::new(
            runtime_pb::InvocationStatus::Succeeded,
        ))
        .await;
        let executor = RuntimeExecutor::connect(runtime_config(format!("http://{addr}")))
            .await
            .unwrap();

        let outcome = executor
            .execute(&proposal_with_runtime_attributes(), &Attributes::empty())
            .await
            .unwrap();

        assert!(outcome.succeeded);
        assert_eq!(outcome.execution_id, "inv-456");
        assert_eq!(outcome.duration, DurationMs::from_millis(321));
        assert_eq!(
            outcome.output.get("runtime.status"),
            Some(&json!("succeeded"))
        );
        assert_eq!(
            outcome.output.get("runtime.tool_name"),
            Some(&json!("fs.write_file"))
        );

        let create_requests = runtime.create_requests.lock().await;
        assert_eq!(create_requests.len(), 1);
        assert_eq!(
            create_requests[0].principal.as_ref().unwrap().tenant_id,
            "tenant-a"
        );
        assert_eq!(
            create_requests[0].metadata.get("runner_profile"),
            Some(&"safe".to_owned())
        );
        assert_eq!(
            create_requests[0].metadata.get("choreographer.proposal_id"),
            Some(&"p1".to_owned())
        );
        drop(create_requests);

        let invoke_requests = runtime.invoke_requests.lock().await;
        assert_eq!(invoke_requests.len(), 1);
        assert_eq!(invoke_requests[0].session_id, "sess-123");
        assert_eq!(invoke_requests[0].tool_name, "fs.write_file");
        assert_eq!(invoke_requests[0].correlation_id, "p1");
        assert!(invoke_requests[0].args.is_some());
        drop(invoke_requests);

        let close_requests = runtime.close_requests.lock().await;
        assert_eq!(close_requests.len(), 1);
        assert_eq!(close_requests[0].session_id, "sess-123");
        drop(close_requests);

        let _ = shutdown.send(());
    }

    #[tokio::test]
    async fn denied_invocation_returns_unsuccessful_execution_outcome() {
        let (addr, shutdown, _runtime) =
            spawn_runtime(RecordingRuntime::new(runtime_pb::InvocationStatus::Denied)).await;
        let executor = RuntimeExecutor::connect(runtime_config(format!("http://{addr}")))
            .await
            .unwrap();

        let outcome = executor
            .execute(&proposal_with_runtime_attributes(), &Attributes::empty())
            .await
            .unwrap();

        assert!(!outcome.succeeded);
        assert_eq!(outcome.output.get("runtime.status"), Some(&json!("denied")));
        assert_eq!(
            outcome.output.get("runtime.error"),
            Some(&json!({"code": "policy_denied", "message": "approval required"}))
        );

        let _ = shutdown.send(());
    }

    #[tokio::test]
    async fn invoke_transport_error_maps_to_domain_error() {
        let (addr, shutdown, _runtime) = spawn_runtime(RecordingRuntime::with_invoke_error(
            tonic::Code::Unavailable,
        ))
        .await;
        let executor = RuntimeExecutor::connect(runtime_config(format!("http://{addr}")))
            .await
            .unwrap();

        let err = executor
            .execute(&proposal_with_runtime_attributes(), &Attributes::empty())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "runtime transport error"
            }
        ));

        let _ = shutdown.send(());
    }

    #[test]
    fn request_mapping_prefers_execution_options_over_proposal_attributes() {
        let proposal = proposal_with_runtime_attributes();
        let options = Attributes::new(BTreeMap::from([
            (KEY_TOOL_NAME.to_owned(), json!("k8s.restart_workload")),
            (KEY_APPROVED.to_owned(), json!(true)),
            (KEY_SESSION_ID.to_owned(), json!("sess-existing")),
        ]))
        .unwrap();

        let request = RuntimeExecutionRequest::from_inputs(
            &proposal,
            &options,
            &RuntimePrincipal {
                tenant_id: "tenant-a".to_owned(),
                actor_id: "actor-a".to_owned(),
                roles: vec!["developer".to_owned()],
            },
        )
        .unwrap();

        assert_eq!(request.tool_name, "k8s.restart_workload");
        assert!(request.approved);
        assert_eq!(request.session_id.as_deref(), Some("sess-existing"));
    }

    #[tokio::test]
    async fn runtime_tls_defaults_to_disabled_on_http_endpoint() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        let tls =
            RuntimeClientTlsConfig::from_env_for_endpoint(Some("http://runtime.example:50053"))
                .unwrap();
        assert_eq!(tls.mode, RuntimeClientTlsMode::Disabled);
        assert_eq!(tls.mode_name(), "disabled");
    }

    #[tokio::test]
    async fn runtime_tls_auto_upgrades_to_server_on_https_endpoint() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        let tls =
            RuntimeClientTlsConfig::from_env_for_endpoint(Some("https://runtime.example.com"))
                .unwrap();
        assert_eq!(tls.mode, RuntimeClientTlsMode::Server);
    }

    #[tokio::test]
    async fn runtime_tls_auto_upgrades_to_mutual_when_client_paths_set() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        std::env::set_var(RUNTIME_TLS_CERT_PATH_ENV, "/tmp/client.crt");
        std::env::set_var(RUNTIME_TLS_KEY_PATH_ENV, "/tmp/client.key");

        let tls =
            RuntimeClientTlsConfig::from_env_for_endpoint(Some("http://runtime.example:50053"))
                .unwrap();
        assert_eq!(tls.mode, RuntimeClientTlsMode::Mutual);
        assert_eq!(
            tls.cert_path.as_deref(),
            Some(std::path::Path::new("/tmp/client.crt"))
        );

        clear_runtime_env();
    }

    #[tokio::test]
    async fn runtime_tls_explicit_mode_wins_over_auto_detection() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        std::env::set_var(RUNTIME_TLS_MODE_ENV, "disabled");
        // https endpoint would otherwise auto-upgrade to server.
        let tls =
            RuntimeClientTlsConfig::from_env_for_endpoint(Some("https://runtime.example.com"))
                .unwrap();
        assert_eq!(tls.mode, RuntimeClientTlsMode::Disabled);
        clear_runtime_env();
    }

    #[tokio::test]
    async fn runtime_tls_mutual_mode_requires_cert_and_key() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        std::env::set_var(RUNTIME_TLS_MODE_ENV, "mutual");
        // cert and key missing.
        let err =
            RuntimeClientTlsConfig::from_env_for_endpoint(Some("https://runtime.example.com"))
                .unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "CHOREO_RUNTIME_TLS_MODE=mutual requires both _CERT_PATH and _KEY_PATH"
            }
        ));
        clear_runtime_env();
    }

    #[tokio::test]
    async fn runtime_tls_invalid_mode_is_rejected() {
        let _guard = ENV_LOCK.lock().await;
        clear_runtime_env();
        std::env::set_var(RUNTIME_TLS_MODE_ENV, "weird");
        let err = RuntimeClientTlsConfig::from_env_for_endpoint(None).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "CHOREO_RUNTIME_TLS_MODE must be one of: disabled, server, mutual"
            }
        ));
        clear_runtime_env();
    }

    #[test]
    fn endpoint_uri_for_tls_mode_rewrites_http_to_https() {
        assert_eq!(
            endpoint_uri_for_tls_mode("http://x.example:50053", RuntimeClientTlsMode::Server),
            "https://x.example:50053"
        );
        assert_eq!(
            endpoint_uri_for_tls_mode("https://x.example", RuntimeClientTlsMode::Mutual),
            "https://x.example"
        );
        assert_eq!(
            endpoint_uri_for_tls_mode("http://x.example", RuntimeClientTlsMode::Disabled),
            "http://x.example"
        );
    }
}
