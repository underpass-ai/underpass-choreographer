use choreo_core::value_objects::CeremonyId;
use serde_json::Value;

use super::embedded_request_fields::required_string;

/// Validated MCP request for one ceremony instance projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EmbeddedGetCeremonyInstanceRequest {
    ceremony_id: CeremonyId,
}

impl EmbeddedGetCeremonyInstanceRequest {
    pub(super) fn into_ceremony_id(self) -> CeremonyId {
        self.ceremony_id
    }
}

impl TryFrom<&Value> for EmbeddedGetCeremonyInstanceRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        Ok(Self {
            ceremony_id: CeremonyId::new(required_string(object, "ceremony_id")?)
                .map_err(|error| error.to_string())?,
        })
    }
}
