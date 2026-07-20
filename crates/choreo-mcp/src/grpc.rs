//! Live-gRPC backend for the choreo MCP adapter.
//!
//! Talks to a running choreographer via the `underpass.choreo.v1`
//! gRPC contract. Every tool maps 1:1 to one RPC; JSON arguments and
//! responses are translated field-for-field by `json_to_proto` and
//! `proto_to_json`, so the MCP layer never drops or flattens fields.

mod channel;
mod json_to_proto;
mod proto_to_json;
mod streaming;
mod tools;

use async_trait::async_trait;
use serde_json::Value;
use tonic::transport::Channel;
use tracing::debug;

use crate::backend::{
    endpoint_uri_for_tls_mode, ChoreoMcpGrpcTlsConfig, ChoreoMcpToolBackend, ChoreoMcpToolFuture,
};

/// gRPC-backed implementation of [`ChoreoMcpToolBackend`].
///
/// Holds a lazily-connected tonic `Channel` (resolved on the first
/// call) and the negotiated TLS posture. The endpoint URI is rewritten
/// to `https://` when TLS is enabled so callers can flip one env var
/// without having to also change the URL scheme.
#[derive(Debug, Clone)]
pub struct GrpcChoreoMcpBackend {
    endpoint: String,
    tls: ChoreoMcpGrpcTlsConfig,
}

impl GrpcChoreoMcpBackend {
    /// Build a backend pointed at `endpoint` with the given TLS posture.
    /// No network call happens here — the connection is opened on the
    /// first tool call so `--help`-style probes don't block on DNS.
    pub fn new(endpoint: impl Into<String>, tls: ChoreoMcpGrpcTlsConfig) -> Self {
        let endpoint = endpoint_uri_for_tls_mode(&endpoint.into(), tls.mode());
        Self { endpoint, tls }
    }

    async fn channel(&self) -> Result<Channel, String> {
        channel::open_channel(&self.endpoint, &self.tls).await
    }
}

#[async_trait]
impl ChoreoMcpToolBackend for GrpcChoreoMcpBackend {
    fn backend_name(&self) -> &'static str {
        "grpc"
    }

    fn grpc_tls_mode_name(&self) -> &'static str {
        self.tls.mode_name()
    }

    fn supports_tool(&self, name: &str) -> bool {
        crate::protocol::is_grpc_tool(name)
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> ChoreoMcpToolFuture<'a> {
        Box::pin(async move {
            debug!(
                tool = name,
                tls = self.tls.mode_name(),
                endpoint = self.endpoint.as_str(),
                "choreo_mcp: dispatching live tool call"
            );
            let channel = self.channel().await?;
            let structured = tools::dispatch(channel, name, arguments).await?;
            Ok(crate::protocol::tool_success_result(structured))
        })
    }
}
