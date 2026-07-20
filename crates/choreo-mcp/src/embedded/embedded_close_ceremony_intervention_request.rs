use choreo_app::usecases::CloseCeremonyInterventionInput;
use choreo_core::value_objects::{CeremonyId, CeremonyInterventionId, RoleId};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{load_instance_definition, required_string};

/// Validated MCP request that closes a dynamic ceremony intervention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedCloseCeremonyInterventionRequest {
    ceremony: CeremonyId,
    intervention: CeremonyInterventionId,
    role: RoleId,
}

impl EmbeddedCloseCeremonyInterventionRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (_, instance) = load_instance_definition(choreographer, &self.ceremony).await?;
        choreographer
            .close_intervention(CloseCeremonyInterventionInput::new(
                self.ceremony.clone(),
                instance.definition_name().clone(),
                instance.definition_version().clone(),
                self.intervention,
                self.role,
            ))
            .await
            .map_err(|error| format!("failed to close ceremony intervention: {error}"))?;
        Ok(self.ceremony)
    }
}

impl TryFrom<&Value> for EmbeddedCloseCeremonyInterventionRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        Ok(Self {
            ceremony: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
            intervention: CeremonyInterventionId::new(required_string(object, "intervention_id")?)
                .map_err(|error| error.to_string())?,
            role: RoleId::new(required_string(object, "role_id")?)
                .map_err(|error| error.to_string())?,
        })
    }
}
