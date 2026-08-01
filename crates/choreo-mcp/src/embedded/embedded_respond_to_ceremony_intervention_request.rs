use choreo_app::usecases::RespondToCeremonyInterventionInput;
use choreo_core::value_objects::{
    AuditActorKind, CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, RoleId,
};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{optional_attributes, required_actor_kind, required_string};

/// Validated MCP request that records one role's intervention response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedRespondToCeremonyInterventionRequest {
    ceremony_id: CeremonyId,
    intervention_id: CeremonyInterventionId,
    role_id: RoleId,
    role_kind: AuditActorKind,
    content: CeremonyInterventionContent,
}

impl EmbeddedRespondToCeremonyInterventionRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        choreographer
            .respond_to_intervention(RespondToCeremonyInterventionInput::new(
                self.ceremony_id.clone(),
                self.intervention_id,
                self.role_id,
                self.role_kind,
                self.content,
            ))
            .await
            .map_err(|error| format!("failed to respond to ceremony intervention: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedRespondToCeremonyInterventionRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        Ok(Self {
            ceremony_id: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
            intervention_id: CeremonyInterventionId::new(required_string(
                object,
                "intervention_id",
            )?)
            .map_err(|error| error.to_string())?,
            role_kind: required_actor_kind(object, "role_kind")?,
            role_id: RoleId::new(required_string(object, "role_id")?)
                .map_err(|error| error.to_string())?,
            content: CeremonyInterventionContent::new(
                required_string(object, "message")?,
                optional_attributes(object, "details")?,
            )
            .map_err(|error| error.to_string())?,
        })
    }
}
