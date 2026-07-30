use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_core::entities::PublicationOutcome;
use choreo_embedded::EmbeddedChoreographer;
use serde_json::Value;

use super::embedded_request_fields::required_string;

/// Validated MCP request to fix a definition to an immutable version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EmbeddedPublishCeremonyDefinitionRequest {
    definition_yaml: String,
}

impl EmbeddedPublishCeremonyDefinitionRequest {
    /// Parse, prove publishable, then publish.
    ///
    /// The draft is analysed on the way through, so a definition that
    /// could never be executed is refused before it occupies a version
    /// — the point of publishing is that what comes back out is known
    /// good.
    pub(crate) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<PublicationOutcome, String> {
        let draft = CeremonyDefinitionYaml::parse_draft_str(&self.definition_yaml)
            .map_err(|error| format!("ceremony draft could not be parsed: {error}"))?;
        let definition = draft
            .publish()
            .map_err(|error| format!("ceremony draft is not publishable: {error}"))?;

        choreographer
            .publish_definition(definition)
            .await
            .map_err(|error| format!("publication failed: {error}"))
    }
}

impl TryFrom<&Value> for EmbeddedPublishCeremonyDefinitionRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        Ok(Self {
            definition_yaml: required_string(object, "definition_yaml")?,
        })
    }
}
