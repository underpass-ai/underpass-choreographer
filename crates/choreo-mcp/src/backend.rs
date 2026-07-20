//! Backend trait + env-driven TLS configuration for the choreo MCP
//! server.
//!
//! The MCP layer talks to exactly one [`ChoreoMcpToolBackend`]; the
//! production impl is gRPC against a running choreographer, the
//! embedded impl runs the ceremony engine in process, and the fixture
//! impl reads canned responses for client-wiring smoke tests.
//! Backend selection happens at startup from
//! [`CHOREO_MCP_BACKEND`](MCP_BACKEND_ENV) — default `grpc`,
//! fail-fast when the endpoint env is missing.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use serde_json::Value;

/// Endpoint URL the MCP gRPC backend should connect to.
pub const GRPC_ENDPOINT_ENV: &str = "CHOREO_MCP_GRPC_ENDPOINT";
/// Backend selector: `grpc` (default), `embedded`, or `fixture`.
pub const MCP_BACKEND_ENV: &str = "CHOREO_MCP_BACKEND";
/// TLS mode override for the gRPC client: `disabled`/`server`/`mutual`.
pub const GRPC_TLS_MODE_ENV: &str = "CHOREO_MCP_GRPC_TLS_MODE";
/// PEM bundle the client should trust as a CA when verifying the
/// server (server or mutual mode).
pub const GRPC_TLS_CA_PATH_ENV: &str = "CHOREO_MCP_GRPC_TLS_CA_PATH";
/// Client certificate PEM the MCP presents to the server (mutual).
pub const GRPC_TLS_CERT_PATH_ENV: &str = "CHOREO_MCP_GRPC_TLS_CERT_PATH";
/// Client private key PEM matching `_CERT_PATH` (mutual).
pub const GRPC_TLS_KEY_PATH_ENV: &str = "CHOREO_MCP_GRPC_TLS_KEY_PATH";
/// Override the TLS SNI/domain (when the URL host differs from the
/// cert CN/SAN, e.g. behind a kube Service).
pub const GRPC_TLS_DOMAIN_NAME_ENV: &str = "CHOREO_MCP_GRPC_TLS_DOMAIN_NAME";

/// Async tool-call future shape. Boxed so the trait stays object-safe.
pub type ChoreoMcpToolFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

/// Single seam between the MCP request dispatcher and any concrete
/// transport. The MCP server reads only the trait; switching from
/// fixtures to live gRPC is a single line in `main.rs`.
pub trait ChoreoMcpToolBackend: Send + Sync {
    /// Operator-friendly backend label for the `initialize` response
    /// metadata and structured tracing.
    fn backend_name(&self) -> &'static str;

    /// Operator-friendly TLS posture label for startup logs and the
    /// `initialize` response metadata.
    fn grpc_tls_mode_name(&self) -> &'static str {
        "disabled"
    }

    /// Whether this backend can execute a catalog tool.
    ///
    /// Full API backends use the default. Focused backends override it
    /// so `tools/list` never advertises operations they cannot honor.
    fn supports_tool(&self, _name: &str) -> bool {
        true
    }

    /// Dispatch one MCP tool call.
    ///
    /// `arguments` is the raw JSON `arguments` field from
    /// `tools/call.params`. The backend owns its own argument
    /// validation, proto mapping, and error taxonomy. Errors come back
    /// as plain strings; the server wraps them in the MCP-spec
    /// `isError: true` content block.
    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> ChoreoMcpToolFuture<'a>;
}

/// Configured gRPC TLS posture for the MCP client.
///
/// Constructed via [`ChoreoMcpGrpcTlsConfig::from_env_for_endpoint`].
/// The MCP layer never decides TLS by itself — every variant maps to
/// a tonic `ClientTlsConfig` shape one of the backends knows how to
/// apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChoreoMcpGrpcTlsConfig {
    pub(crate) mode: ChoreoMcpGrpcTlsMode,
    pub(crate) ca_path: Option<PathBuf>,
    pub(crate) cert_path: Option<PathBuf>,
    pub(crate) key_path: Option<PathBuf>,
    pub(crate) domain_name: Option<String>,
}

/// Operator-visible TLS posture options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChoreoMcpGrpcTlsMode {
    /// Plain HTTP/2 over TCP.
    Disabled,
    /// One-way TLS: the server presents an identity, the client
    /// verifies it through a CA bundle (or system roots).
    Server,
    /// Mutual TLS: client also presents an identity.
    Mutual,
}

impl ChoreoMcpGrpcTlsMode {
    /// Stable label for logs/metadata. Kept distinct from `Debug` so
    /// machine consumers can grep for it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Server => "server",
            Self::Mutual => "mutual",
        }
    }
}

impl ChoreoMcpGrpcTlsConfig {
    /// Explicitly disabled TLS — useful for in-cluster talks behind
    /// network policies and for unit tests.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            mode: ChoreoMcpGrpcTlsMode::Disabled,
            ca_path: None,
            cert_path: None,
            key_path: None,
            domain_name: None,
        }
    }

    /// One-way TLS with a caller-supplied CA bundle. Use system roots
    /// by passing an `https://` endpoint without setting `_CA_PATH`.
    #[must_use]
    pub fn server(ca_path: impl Into<PathBuf>, domain_name: Option<String>) -> Self {
        Self {
            mode: ChoreoMcpGrpcTlsMode::Server,
            ca_path: Some(ca_path.into()),
            cert_path: None,
            key_path: None,
            domain_name,
        }
    }

    /// Mutual TLS: caller presents an identity in addition to
    /// verifying the server.
    #[must_use]
    pub fn mutual(
        ca_path: impl Into<PathBuf>,
        cert_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        domain_name: Option<String>,
    ) -> Self {
        Self {
            mode: ChoreoMcpGrpcTlsMode::Mutual,
            ca_path: Some(ca_path.into()),
            cert_path: Some(cert_path.into()),
            key_path: Some(key_path.into()),
            domain_name,
        }
    }

    /// Read the TLS posture from environment, with helpful
    /// auto-detection so the common cases stay one env var:
    ///
    /// - `https://` endpoint OR `_CA_PATH` set OR `_DOMAIN_NAME` set
    ///   → server TLS (system roots if no CA path);
    /// - `_CERT_PATH` and/or `_KEY_PATH` set → mutual TLS;
    /// - explicit `CHOREO_MCP_GRPC_TLS_MODE` always wins.
    #[must_use]
    pub fn from_env_for_endpoint(endpoint: Option<&str>) -> Self {
        let ca_path = optional_env_path(GRPC_TLS_CA_PATH_ENV);
        let cert_path = optional_env_path(GRPC_TLS_CERT_PATH_ENV);
        let key_path = optional_env_path(GRPC_TLS_KEY_PATH_ENV);
        let domain_name = optional_env_string(GRPC_TLS_DOMAIN_NAME_ENV);
        let server_tls_requested = ca_path.is_some()
            || domain_name.is_some()
            || endpoint.is_some_and(|endpoint| endpoint.trim().starts_with("https://"));
        let mode = optional_env_string(GRPC_TLS_MODE_ENV)
            .and_then(|value| parse_tls_mode(&value))
            .unwrap_or_else(|| {
                if cert_path.is_some() || key_path.is_some() {
                    ChoreoMcpGrpcTlsMode::Mutual
                } else if server_tls_requested {
                    ChoreoMcpGrpcTlsMode::Server
                } else {
                    ChoreoMcpGrpcTlsMode::Disabled
                }
            });

        Self {
            mode,
            ca_path,
            cert_path,
            key_path,
            domain_name,
        }
    }

    /// Convenience: read TLS posture from env with no extra context.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_env_for_endpoint(std::env::var(GRPC_ENDPOINT_ENV).ok().as_deref())
    }

    /// Active mode.
    #[must_use]
    pub fn mode(&self) -> ChoreoMcpGrpcTlsMode {
        self.mode
    }

    /// Stable label for logs/metadata.
    #[must_use]
    pub fn mode_name(&self) -> &'static str {
        self.mode.as_str()
    }
}

/// When TLS is enabled, automatically rewrite an `http://` endpoint to
/// `https://` so callers can flip a single env var (the TLS knob)
/// without having to also change the URL scheme.
#[cfg(any(feature = "grpc", test))]
pub(crate) fn endpoint_uri_for_tls_mode(endpoint: &str, mode: ChoreoMcpGrpcTlsMode) -> String {
    if mode == ChoreoMcpGrpcTlsMode::Disabled {
        return endpoint.to_string();
    }
    endpoint.strip_prefix("http://").map_or_else(
        || endpoint.to_string(),
        |without_scheme| format!("https://{without_scheme}"),
    )
}

fn parse_tls_mode(value: &str) -> Option<ChoreoMcpGrpcTlsMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" | "disable" | "off" | "false" | "none" => Some(ChoreoMcpGrpcTlsMode::Disabled),
        "server" | "tls" => Some(ChoreoMcpGrpcTlsMode::Server),
        "mutual" | "mtls" | "m-tls" => Some(ChoreoMcpGrpcTlsMode::Mutual),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_mode_labels_are_stable() {
        assert_eq!(ChoreoMcpGrpcTlsMode::Disabled.as_str(), "disabled");
        assert_eq!(ChoreoMcpGrpcTlsMode::Server.as_str(), "server");
        assert_eq!(ChoreoMcpGrpcTlsMode::Mutual.as_str(), "mutual");
    }

    #[test]
    fn endpoint_uri_upgrades_http_when_tls_enabled() {
        assert_eq!(
            endpoint_uri_for_tls_mode("http://127.0.0.1:50055", ChoreoMcpGrpcTlsMode::Server),
            "https://127.0.0.1:50055"
        );
        assert_eq!(
            endpoint_uri_for_tls_mode("https://x.example", ChoreoMcpGrpcTlsMode::Mutual),
            "https://x.example"
        );
        assert_eq!(
            endpoint_uri_for_tls_mode("http://127.0.0.1:50055", ChoreoMcpGrpcTlsMode::Disabled),
            "http://127.0.0.1:50055"
        );
    }

    #[test]
    fn parse_tls_mode_accepts_aliases() {
        assert_eq!(
            parse_tls_mode("disabled"),
            Some(ChoreoMcpGrpcTlsMode::Disabled)
        );
        assert_eq!(parse_tls_mode("none"), Some(ChoreoMcpGrpcTlsMode::Disabled));
        assert_eq!(parse_tls_mode("server"), Some(ChoreoMcpGrpcTlsMode::Server));
        assert_eq!(parse_tls_mode("tls"), Some(ChoreoMcpGrpcTlsMode::Server));
        assert_eq!(parse_tls_mode("mutual"), Some(ChoreoMcpGrpcTlsMode::Mutual));
        assert_eq!(parse_tls_mode("mtls"), Some(ChoreoMcpGrpcTlsMode::Mutual));
        assert_eq!(parse_tls_mode("garbage"), None);
    }

    #[test]
    fn tls_constructors_preserve_paths() {
        let server = ChoreoMcpGrpcTlsConfig::server("/tmp/ca.pem", Some("choreo.local".into()));
        assert_eq!(server.mode(), ChoreoMcpGrpcTlsMode::Server);
        assert_eq!(
            server.ca_path.as_deref(),
            Some(std::path::Path::new("/tmp/ca.pem"))
        );
        assert_eq!(server.domain_name.as_deref(), Some("choreo.local"));

        let mutual =
            ChoreoMcpGrpcTlsConfig::mutual("/tmp/ca.pem", "/tmp/cert.pem", "/tmp/key.pem", None);
        assert_eq!(mutual.mode(), ChoreoMcpGrpcTlsMode::Mutual);
        assert_eq!(
            mutual.cert_path.as_deref(),
            Some(std::path::Path::new("/tmp/cert.pem"))
        );
        assert_eq!(
            mutual.key_path.as_deref(),
            Some(std::path::Path::new("/tmp/key.pem"))
        );
    }
}
