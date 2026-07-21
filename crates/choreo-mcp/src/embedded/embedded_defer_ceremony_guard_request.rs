use choreo_app::usecases::DeferCeremonyGuardInput;
use choreo_core::value_objects::{CeremonyGuardDeferralContent, CeremonyId, GuardName};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{load_instance_definition, required_string, required_strings};

/// Validated MCP request for one explicit human guard deferral.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedDeferCeremonyGuardRequest {
    ceremony_id: CeremonyId,
    guard_name: GuardName,
    content: CeremonyGuardDeferralContent,
}

impl EmbeddedDeferCeremonyGuardRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (_, instance) = load_instance_definition(choreographer, &self.ceremony_id).await?;
        choreographer
            .defer_guard(DeferCeremonyGuardInput::new(
                self.ceremony_id.clone(),
                instance.definition_name().clone(),
                instance.definition_version().clone(),
                self.guard_name,
                self.content,
            ))
            .await
            .map_err(|error| format!("failed to defer ceremony guard: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedDeferCeremonyGuardRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        Ok(Self {
            ceremony_id: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
            guard_name: GuardName::new(required_string(object, "guard_name")?)
                .map_err(|error| error.to_string())?,
            content: CeremonyGuardDeferralContent::new(
                required_string(object, "statement")?,
                required_string(object, "reason")?,
                required_strings(object, "reconsider_when")?,
            )
            .map_err(|error| error.to_string())?,
        })
    }
}
