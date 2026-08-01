use choreo_app::usecases::StartCeremonyInput;
use choreo_core::value_objects::{AuditActorKind, CeremonyContext, CeremonyId};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;
use uuid::Uuid;

use super::embedded_request_fields::{
    context_from_json, optional_string, required_actor_kind, required_string,
};

/// Validated MCP request that starts, but does not advance, a ceremony.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedStartCeremonyRequest {
    definition_yaml: String,
    ceremony_id: CeremonyId,
    context: CeremonyContext,
    actor_id: String,
    actor_kind: AuditActorKind,
}

impl EmbeddedStartCeremonyRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let mounted = choreographer
            .mount_yaml(&self.definition_yaml)
            .await
            .map_err(|error| format!("invalid ceremony definition: {error}"))?;
        let definition = mounted
            .definitions()
            .first()
            .ok_or_else(|| "ceremony definition source returned no definitions".to_owned())?;
        let instance = choreographer
            .start(StartCeremonyInput::new(
                self.ceremony_id,
                definition.name().clone(),
                definition.version().clone(),
                self.context,
                self.actor_id,
                self.actor_kind,
            ))
            .await
            .map_err(|error| format!("failed to start ceremony: {error}"))?;
        Ok(instance.id().clone())
    }
}

impl TryFrom<&Value> for EmbeddedStartCeremonyRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        let ceremony_id =
            optional_string(object, "ceremony_id")?.unwrap_or_else(|| Uuid::new_v4().to_string());
        let context = object
            .get("context")
            .map_or_else(|| Ok(CeremonyContext::empty()), context_from_json)?;

        Ok(Self {
            definition_yaml: required_string(object, "definition_yaml")?,
            ceremony_id: CeremonyId::new(ceremony_id).map_err(|error| error.to_string())?,
            context,
            actor_id: required_string(object, "actor_id")?,
            actor_kind: required_actor_kind(object, "actor_kind")?,
        })
    }
}
