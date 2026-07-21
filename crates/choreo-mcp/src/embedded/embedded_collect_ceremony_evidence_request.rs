use choreo_app::usecases::CollectCeremonyEvidenceInput;
use choreo_core::value_objects::{
    CeremonyEvidenceSourceId, CeremonyId, CeremonyInterventionContent, CeremonyInterventionId,
    RoleId,
};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::{
    load_instance_definition, optional_attributes, required_string,
};

/// Validated MCP request that collects evidence for one open intervention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedCollectCeremonyEvidenceRequest {
    ceremony_id: CeremonyId,
    intervention_id: CeremonyInterventionId,
    role_id: RoleId,
    source_id: CeremonyEvidenceSourceId,
    query: CeremonyInterventionContent,
}

impl EmbeddedCollectCeremonyEvidenceRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (_, instance) = load_instance_definition(choreographer, &self.ceremony_id).await?;
        choreographer
            .collect_evidence(CollectCeremonyEvidenceInput::new(
                self.ceremony_id.clone(),
                instance.definition_name().clone(),
                instance.definition_version().clone(),
                self.intervention_id,
                self.role_id,
                self.source_id,
                self.query,
            ))
            .await
            .map_err(|error| format!("failed to collect ceremony evidence: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedCollectCeremonyEvidenceRequest {
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
            role_id: RoleId::new(required_string(object, "role_id")?)
                .map_err(|error| error.to_string())?,
            source_id: CeremonyEvidenceSourceId::new(required_string(object, "source_id")?)
                .map_err(|error| error.to_string())?,
            query: CeremonyInterventionContent::new(
                required_string(object, "query")?,
                optional_attributes(object, "details")?,
            )
            .map_err(|error| error.to_string())?,
        })
    }
}
