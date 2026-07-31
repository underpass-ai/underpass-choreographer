use choreo_app::usecases::ApproveCeremonyGuardInput;
use choreo_core::value_objects::{CeremonyId, GuardCondition, GuardName, RoleId};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{load_instance_definition, required_string};

/// Validated MCP request for one explicit human guard approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedApproveCeremonyGuardRequest {
    ceremony_id: CeremonyId,
    guard_name: GuardName,
    role_id: RoleId,
}

impl EmbeddedApproveCeremonyGuardRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (definition, instance) =
            load_instance_definition(choreographer, &self.ceremony_id).await?;
        let guard = definition
            .guards()
            .get(&self.guard_name)
            .ok_or_else(|| "ceremony guard was not found in the instance definition".to_owned())?;
        if !matches!(guard.condition(), GuardCondition::HumanApproval) {
            return Err("only human approval guards can be approved explicitly".to_owned());
        }
        let is_currently_required = definition
            .available_transitions(instance.current_state())
            .any(|transition| transition.required_guards().contains(&self.guard_name));
        if !is_currently_required {
            return Err(
                "human guard is not required by a transition from the current state".to_owned(),
            );
        }

        choreographer
            .approve_guard(ApproveCeremonyGuardInput::new(
                self.ceremony_id.clone(),
                self.guard_name,
                self.role_id,
            ))
            .await
            .map_err(|error| format!("failed to approve ceremony guard: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedApproveCeremonyGuardRequest {
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
            role_id: RoleId::new(required_string(object, "role_id")?)
                .map_err(|error| error.to_string())?,
        })
    }
}
