use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_core::entities::CeremonyDefinitionDraft;
use serde_json::Value;

use super::embedded_request_fields::required_string;

/// Validated MCP request carrying a ceremony draft to analyse.
///
/// Authoring is read-only: the request holds the draft and nothing
/// else, and answering it never touches the choreographer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EmbeddedCeremonyDraftRequest {
    definition_yaml: String,
}

impl EmbeddedCeremonyDraftRequest {
    /// Parse the draft.
    ///
    /// A failure here is a syntax or identifier failure, not a
    /// structural one: a draft that parses is always analysable, even
    /// when it could never be published.
    pub(crate) fn parse(&self) -> Result<CeremonyDefinitionDraft, String> {
        CeremonyDefinitionYaml::parse_draft_str(&self.definition_yaml)
            .map_err(|error| format!("ceremony draft could not be parsed: {error}"))
    }
}

impl TryFrom<&Value> for EmbeddedCeremonyDraftRequest {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_missing_definition_is_rejected() {
        let error = EmbeddedCeremonyDraftRequest::try_from(&json!({})).unwrap_err();

        assert!(error.contains("definition_yaml"), "{error}");
    }

    #[test]
    fn an_unpublishable_draft_still_parses() {
        let request = EmbeddedCeremonyDraftRequest::try_from(&json!({
            "definition_yaml": UNPUBLISHABLE,
        }))
        .unwrap();

        let draft = request.parse().expect("a parseable draft");

        assert!(!draft.analyze().is_valid());
    }

    const UNPUBLISHABLE: &str = r#"
version: "1.0"
name: "broken_ceremony"
inputs:
  required: []
  optional: []
outputs: {}
states:
  - id: DRAFTING
    initial: true
    terminal: false
  - id: DONE
    initial: false
    terminal: true
transitions:
  - from: DRAFTING
    to: NOWHERE
    trigger: "finish"
    guards: []
steps: []
roles: []
"#;
}
