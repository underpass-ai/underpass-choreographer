//! Stdio MCP adapter for the Underpass Choreographer.
//!
//! Exposes Choreographer capabilities as MCP tools over JSON-RPC 2.0
//! on stdin/stdout. The default backend maps every
//! `underpass.choreo.v1` RPC to a running service; the optional
//! `embedded` backend executes the ceremony engine in process.
//!
//! See `crates/choreo-mcp/README.md` for end-user installation, and
//! `docs/operations/mcp-stdio.md` for the canonical UX.

pub mod backend;
#[cfg(feature = "embedded")]
pub mod embedded;
pub mod fixture;
#[cfg(feature = "grpc")]
pub mod grpc;
pub mod mcp_server_identity;
pub mod observability;
pub mod protocol;
pub mod server;

pub use backend::{
    ChoreoMcpGrpcTlsConfig, ChoreoMcpGrpcTlsMode, ChoreoMcpToolBackend, GRPC_ENDPOINT_ENV,
    GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_DOMAIN_NAME_ENV, GRPC_TLS_KEY_PATH_ENV,
    GRPC_TLS_MODE_ENV, MCP_BACKEND_ENV,
};
#[cfg(feature = "embedded")]
pub use embedded::EmbeddedChoreoMcpBackend;
pub use fixture::FixtureChoreoMcpBackend;
#[cfg(feature = "grpc")]
pub use grpc::GrpcChoreoMcpBackend;
pub use mcp_server_identity::McpServerIdentity;
pub use server::ChoreoMcpServer;
