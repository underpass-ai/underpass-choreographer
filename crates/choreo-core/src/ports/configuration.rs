//! [`ConfigurationPort`] — read-only access to validated service
//! configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Minimal configuration surface the core needs to reason about the
/// service. Adapter implementations (env vars, Figment, Kubernetes
/// downward API, …) map to this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub grpc_port: u16,
    pub http_port: u16,
    pub nats_enabled: bool,
    pub nats_url: String,
    pub trigger_subject: String,
    pub publish_prefix: String,
    /// When set, deliberations persist to Postgres; otherwise the
    /// in-memory repository is wired. Empty-string is treated as
    /// unset so the chart can carry a placeholder default.
    pub postgres_url: Option<String>,
    /// When set, ceremony state, its audit journal and its outbox
    /// persist to an embedded store at this path; otherwise they are
    /// held in memory.
    ///
    /// Unset is a deliberate choice, not a default that happens to be
    /// safe: step leases, idempotency keys and pending human guards
    /// exist to survive failure, and in memory they survive nothing.
    /// A server left volatile says so at startup.
    pub ceremony_store_path: Option<String>,
    /// Transport security for the gRPC server.
    pub grpc_tls: GrpcTlsConfig,
}

/// Mode + paths for the gRPC server's transport security. Validated
/// at load time so the adapter never sees an internally inconsistent
/// state (e.g. mode=server with no cert path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrpcTlsConfig {
    /// Plain HTTP/2 over TCP. Acceptable inside a private mesh; not
    /// for cross-network deployments.
    Disabled,
    /// One-way TLS: the server presents an identity, the client
    /// verifies it via a trust anchor it already has.
    Server { cert_path: String, key_path: String },
    /// Mutual TLS: client presents an identity that the server
    /// validates against `client_ca_path`.
    Mutual {
        cert_path: String,
        key_path: String,
        client_ca_path: String,
    },
}

impl GrpcTlsConfig {
    /// Disabled by default — every other mode requires explicit
    /// configuration with file paths the binary can actually read.
    #[must_use]
    pub fn disabled() -> Self {
        Self::Disabled
    }

    /// Operator-friendly name for the active mode. Useful for
    /// startup-log honesty so deployments expose what they're doing.
    #[must_use]
    pub fn mode_name(&self) -> &'static str {
        match self {
            Self::Disabled => "none",
            Self::Server { .. } => "server",
            Self::Mutual { .. } => "mutual",
        }
    }
}

impl Default for GrpcTlsConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

#[async_trait]
pub trait ConfigurationPort: Send + Sync {
    async fn load(&self) -> Result<ServiceConfig, DomainError>;
}
