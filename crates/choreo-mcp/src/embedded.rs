//! Embedded MCP backend.

mod embedded_apply_ceremony_transition_request;
mod embedded_approve_ceremony_guard_request;
mod embedded_ceremony_instance_presenter;
mod embedded_get_ceremony_instance_request;
mod embedded_request_fields;
mod embedded_run_ceremony_presenter;
mod embedded_run_ceremony_request;
mod embedded_run_ceremony_step_request;
mod embedded_start_ceremony_request;

use choreo_core::value_objects::CeremonyId;
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use crate::backend::{ChoreoMcpToolBackend, ChoreoMcpToolFuture};
use crate::protocol::{
    tool_success_result, APPLY_CEREMONY_TRANSITION_TOOL, APPROVE_CEREMONY_GUARD_TOOL,
    GET_CEREMONY_INSTANCE_TOOL, RUN_CEREMONY_STEP_TOOL, RUN_CEREMONY_TOOL, START_CEREMONY_TOOL,
};

use self::embedded_apply_ceremony_transition_request::EmbeddedApplyCeremonyTransitionRequest;
use self::embedded_approve_ceremony_guard_request::EmbeddedApproveCeremonyGuardRequest;
use self::embedded_ceremony_instance_presenter::EmbeddedCeremonyInstancePresenter;
use self::embedded_get_ceremony_instance_request::EmbeddedGetCeremonyInstanceRequest;
use self::embedded_run_ceremony_presenter::EmbeddedRunCeremonyPresenter;
use self::embedded_run_ceremony_request::EmbeddedRunCeremonyRequest;
use self::embedded_run_ceremony_step_request::EmbeddedRunCeremonyStepRequest;
use self::embedded_start_ceremony_request::EmbeddedStartCeremonyRequest;

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

    async fn present_instance(&self, ceremony_id: &CeremonyId) -> Result<Value, String> {
        EmbeddedCeremonyInstancePresenter::present(&self.choreographer, ceremony_id)
            .await
            .map(tool_success_result)
    }
}

impl ChoreoMcpToolBackend for EmbeddedChoreoMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn supports_tool(&self, name: &str) -> bool {
        matches!(
            name,
            RUN_CEREMONY_TOOL
                | START_CEREMONY_TOOL
                | RUN_CEREMONY_STEP_TOOL
                | APPROVE_CEREMONY_GUARD_TOOL
                | APPLY_CEREMONY_TRANSITION_TOOL
                | GET_CEREMONY_INSTANCE_TOOL
        )
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> ChoreoMcpToolFuture<'a> {
        Box::pin(async move {
            match name {
                RUN_CEREMONY_TOOL => {
                    let request = EmbeddedRunCeremonyRequest::try_from(arguments)?;
                    let output = request.execute(&self.choreographer).await?;
                    Ok(tool_success_result(EmbeddedRunCeremonyPresenter::present(
                        &output,
                    )))
                }
                START_CEREMONY_TOOL => {
                    let request = EmbeddedStartCeremonyRequest::try_from(arguments)?;
                    let ceremony_id = request.execute(&self.choreographer).await?;
                    self.present_instance(&ceremony_id).await
                }
                RUN_CEREMONY_STEP_TOOL => {
                    let request = EmbeddedRunCeremonyStepRequest::try_from(arguments)?;
                    let ceremony_id = request.execute(&self.choreographer).await?;
                    self.present_instance(&ceremony_id).await
                }
                APPROVE_CEREMONY_GUARD_TOOL => {
                    let request = EmbeddedApproveCeremonyGuardRequest::try_from(arguments)?;
                    let ceremony_id = request.execute(&self.choreographer).await?;
                    self.present_instance(&ceremony_id).await
                }
                APPLY_CEREMONY_TRANSITION_TOOL => {
                    let request = EmbeddedApplyCeremonyTransitionRequest::try_from(arguments)?;
                    let ceremony_id = request.execute(&self.choreographer).await?;
                    self.present_instance(&ceremony_id).await
                }
                GET_CEREMONY_INSTANCE_TOOL => {
                    let request = EmbeddedGetCeremonyInstanceRequest::try_from(arguments)?;
                    let ceremony_id = request.into_ceremony_id();
                    self.present_instance(&ceremony_id).await
                }
                _ => Err(format!("embedded backend: unsupported tool `{name}`")),
            }
        })
    }
}
