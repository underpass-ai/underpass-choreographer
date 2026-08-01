use choreo_app::usecases::ApplyCeremonyTransitionInput;
use choreo_core::value_objects::{AuditActorKind, CeremonyId, TransitionTrigger};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{
    load_instance_definition, required_actor_kind, required_string,
};

/// Validated MCP request that applies one enabled transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedApplyCeremonyTransitionRequest {
    ceremony_id: CeremonyId,
    trigger: TransitionTrigger,
    actor_kind: AuditActorKind,
}

impl EmbeddedApplyCeremonyTransitionRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (definition, _instance) =
            load_instance_definition(choreographer, &self.ceremony_id).await?;
        // The seat is the definition's to say; what filled it is the
        // caller's, and taking both from the definition would record a
        // kind nobody declared.
        let role_id = definition
            .role_id_for_transition(&self.trigger)
            .map_err(|error| format!("ceremony transition has no authorized role: {error}"))?;
        choreographer
            .apply_transition(ApplyCeremonyTransitionInput::new(
                self.ceremony_id.clone(),
                role_id,
                self.actor_kind,
                self.trigger,
            ))
            .await
            .map_err(|error| format!("failed to apply ceremony transition: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedApplyCeremonyTransitionRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        Ok(Self {
            ceremony_id: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
            trigger: TransitionTrigger::new(required_string(object, "trigger")?)
                .map_err(|error| error.to_string())?,
            actor_kind: required_actor_kind(object, "actor_kind")?,
        })
    }
}
