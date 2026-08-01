use std::collections::BTreeMap;

use choreo_app::usecases::BindCeremonyParticipantsInput;
use choreo_core::entities::CeremonyInstance;
use choreo_core::value_objects::{CeremonyId, RoleId, Specialty};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{required_actor_kind, required_string};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedBindCeremonyParticipantsRequest {
    input: BindCeremonyParticipantsInput,
}

impl EmbeddedBindCeremonyParticipantsRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyInstance, String> {
        choreographer
            .bind_participants(self.input)
            .await
            .map_err(|error| format!("failed to seat ceremony participants: {error}"))
    }
}

impl TryFrom<&Value> for EmbeddedBindCeremonyParticipantsRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        let seating = object
            .get("seating")
            .and_then(Value::as_object)
            .ok_or_else(|| "field `seating` must be an object".to_owned())?;

        let mut seats = BTreeMap::new();
        for (role, specialty) in seating {
            let specialty = specialty
                .as_str()
                .ok_or_else(|| format!("`seating.{role}` must be a string"))?;
            seats.insert(
                RoleId::new(role.clone()).map_err(|error| error.to_string())?,
                Specialty::new(specialty).map_err(|error| error.to_string())?,
            );
        }

        Ok(Self {
            input: BindCeremonyParticipantsInput::new(
                CeremonyId::new(required_string(object, "ceremony_id")?)
                    .map_err(|error| error.to_string())?,
                seats,
                required_string(object, "actor_id")?,
                required_actor_kind(object, "actor_kind")?,
            )
            .map_err(|error| error.to_string())?,
        })
    }
}
