use choreo_app::usecases::RequestCeremonyInterventionInput;
use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
    CeremonyInterventionProvenance, CeremonyInterventionTarget, RoleId,
};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;
use uuid::Uuid;

use super::embedded_request_fields::{
    load_instance_definition, optional_attributes, optional_role_ids, optional_string,
    required_string,
};

/// Validated MCP request that opens a dynamic ceremony intervention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedRequestCeremonyInterventionRequest {
    ceremony_id: CeremonyId,
    intervention_id: CeremonyInterventionId,
    role_id: RoleId,
    kind: CeremonyInterventionKind,
    target: CeremonyInterventionTarget,
    content: CeremonyInterventionContent,
    provenance: Option<CeremonyInterventionProvenance>,
}

impl EmbeddedRequestCeremonyInterventionRequest {
    pub(super) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<CeremonyId, String> {
        let (_, instance) = load_instance_definition(choreographer, &self.ceremony_id).await?;
        let mut input = RequestCeremonyInterventionInput::new(
            self.ceremony_id.clone(),
            instance.definition_name().clone(),
            instance.definition_version().clone(),
            self.intervention_id,
            self.role_id,
            self.kind,
            self.target,
            self.content,
        );
        if let Some(provenance) = self.provenance {
            input = input.with_provenance(provenance);
        }
        choreographer
            .request_intervention(input)
            .await
            .map_err(|error| format!("failed to request ceremony intervention: {error}"))?;
        Ok(self.ceremony_id)
    }
}

impl TryFrom<&Value> for EmbeddedRequestCeremonyInterventionRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        let intervention_id = optional_string(object, "intervention_id")?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let target = match optional_role_ids(object, "target_role_ids")? {
            Some(role_ids) => CeremonyInterventionTarget::roles(role_ids),
            None => Ok(CeremonyInterventionTarget::table()),
        }
        .map_err(|error| error.to_string())?;

        Ok(Self {
            ceremony_id: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
            intervention_id: CeremonyInterventionId::new(intervention_id)
                .map_err(|error| error.to_string())?,
            role_id: RoleId::new(required_string(object, "role_id")?)
                .map_err(|error| error.to_string())?,
            kind: parse_kind(&required_string(object, "kind")?)?,
            target,
            content: CeremonyInterventionContent::new(
                required_string(object, "message")?,
                optional_attributes(object, "details")?,
            )
            .map_err(|error| error.to_string())?,
            provenance: optional_provenance(object)?,
        })
    }
}

fn optional_provenance(
    object: &serde_json::Map<String, Value>,
) -> Result<Option<CeremonyInterventionProvenance>, String> {
    let Some(value) = object.get("provenance") else {
        return Ok(None);
    };
    let provenance = value
        .as_object()
        .ok_or_else(|| "field `provenance` must be an object".to_owned())?;
    Ok(Some(CeremonyInterventionProvenance::selected_from(
        CeremonyInterventionId::new(required_string(provenance, "source_intervention_id")?)
            .map_err(|error| error.to_string())?,
        RoleId::new(required_string(provenance, "source_response_role_id")?)
            .map_err(|error| error.to_string())?,
        RoleId::new(required_string(provenance, "selected_role_id")?)
            .map_err(|error| error.to_string())?,
    )))
}

fn parse_kind(raw: &str) -> Result<CeremonyInterventionKind, String> {
    match raw {
        "opinion" => Ok(CeremonyInterventionKind::Opinion),
        "investigation" => Ok(CeremonyInterventionKind::Investigation),
        "action" => Ok(CeremonyInterventionKind::Action),
        _ => Err("field `kind` must be one of: opinion, investigation, action".to_owned()),
    }
}
