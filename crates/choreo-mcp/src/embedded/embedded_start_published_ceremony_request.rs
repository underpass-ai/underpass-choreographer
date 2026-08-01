use choreo_app::usecases::StartCeremonyInput;
use choreo_core::value_objects::{
    AuditActorKind, CeremonyContext, CeremonyId, CeremonyName, CeremonyVersion,
};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;
use uuid::Uuid;

use super::embedded_request_fields::{
    context_from_json, optional_string, required_actor_kind, required_string,
};

/// Validated MCP request that starts a ceremony from a published
/// definition.
///
/// It names a version instead of carrying a document. That is the whole
/// difference: the instance is bound to a definition that can be looked
/// up and checked afterwards, rather than to one that existed only in
/// the call that started it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedStartPublishedCeremonyRequest {
    ceremony_id: CeremonyId,
    definition_name: CeremonyName,
    definition_version: CeremonyVersion,
    context: CeremonyContext,
    actor_id: String,
    actor_kind: AuditActorKind,
}

impl EmbeddedStartPublishedCeremonyRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let instance = choreographer
            .start_published(StartCeremonyInput::new(
                self.ceremony_id,
                self.definition_name,
                self.definition_version,
                self.context,
                self.actor_id,
                self.actor_kind,
            ))
            .await
            .map_err(|error| format!("failed to start published ceremony: {error}"))?;
        Ok(instance.id().clone())
    }
}

impl TryFrom<&Value> for EmbeddedStartPublishedCeremonyRequest {
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
            ceremony_id: CeremonyId::new(ceremony_id).map_err(|error| error.to_string())?,
            definition_name: CeremonyName::new(required_string(object, "ceremony")?)
                .map_err(|error| error.to_string())?,
            definition_version: CeremonyVersion::new(required_string(object, "version")?)
                .map_err(|error| error.to_string())?,
            context,
            actor_id: required_string(object, "actor_id")?,
            actor_kind: required_actor_kind(object, "actor_kind")?,
        })
    }
}
