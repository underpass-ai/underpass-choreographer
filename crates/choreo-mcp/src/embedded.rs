//! Embedded MCP backend.

mod embedded_run_ceremony_presenter;
mod embedded_run_ceremony_request;

use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use crate::backend::{ChoreoMcpToolBackend, ChoreoMcpToolFuture};
use crate::protocol::tool_success_result;

use self::embedded_run_ceremony_presenter::EmbeddedRunCeremonyPresenter;
use self::embedded_run_ceremony_request::EmbeddedRunCeremonyRequest;

const RUN_CEREMONY_TOOL: &str = "choreo_run_ceremony";

/// MCP adapter that executes ceremonies inside the host process.
#[derive(Clone, Debug, Default)]
pub struct EmbeddedChoreoMcpBackend {
    choreographer: EmbeddedChoreographer,
}

impl EmbeddedChoreoMcpBackend {
    #[must_use]
    pub fn new(choreographer: EmbeddedChoreographer) -> Self {
        Self { choreographer }
    }
}

impl ChoreoMcpToolBackend for EmbeddedChoreoMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn supports_tool(&self, name: &str) -> bool {
        name == RUN_CEREMONY_TOOL
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> ChoreoMcpToolFuture<'a> {
        Box::pin(async move {
            if name != RUN_CEREMONY_TOOL {
                return Err(format!("embedded backend: unsupported tool `{name}`"));
            }

            let request = EmbeddedRunCeremonyRequest::try_from(arguments)?;
            let output = request.execute(&self.choreographer).await?;
            Ok(tool_success_result(EmbeddedRunCeremonyPresenter::present(
                &output,
            )))
        })
    }
}
