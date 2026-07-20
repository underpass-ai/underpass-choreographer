use choreo_app::usecases::{RunCeremonyInput, RunCeremonyOutput};
use choreo_core::value_objects::{CeremonyContext, CeremonyId, DurationMs, LeaseOwnerId};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::{Map, Value};
use uuid::Uuid;

const DEFAULT_LEASE_OWNER_ID: &str = "choreo-mcp-embedded";
const DEFAULT_LEASE_TTL_MS: u64 = 30_000;

/// Validated MCP request for an embedded ceremony execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EmbeddedRunCeremonyRequest {
    definition_yaml: String,
    ceremony_id: CeremonyId,
    context: CeremonyContext,
    lease_owner_id: LeaseOwnerId,
    lease_ttl: DurationMs,
}

impl EmbeddedRunCeremonyRequest {
    pub(crate) async fn execute(
        self,
        choreographer: &EmbeddedChoreographer,
    ) -> Result<RunCeremonyOutput, String> {
        let mounted = choreographer
            .mount_yaml(&self.definition_yaml)
            .await
            .map_err(|error| format!("invalid ceremony definition: {error}"))?;
        let definition = mounted
            .definitions()
            .first()
            .cloned()
            .ok_or_else(|| "ceremony definition source returned no definitions".to_owned())?;

        choreographer
            .run(RunCeremonyInput::new(
                self.ceremony_id,
                definition,
                self.context,
                self.lease_owner_id,
                self.lease_ttl,
            ))
            .await
            .map_err(|error| format!("ceremony execution failed: {error}"))
    }
}

impl TryFrom<&Value> for EmbeddedRunCeremonyRequest {
    type Error = String;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        let object = value
            .as_object()
            .ok_or_else(|| "tools/call.arguments must be an object".to_owned())?;
        let definition_yaml = required_string(object, "definition_yaml")?;
        let ceremony_id =
            optional_string(object, "ceremony_id")?.unwrap_or_else(|| Uuid::new_v4().to_string());
        let lease_owner_id = optional_string(object, "lease_owner_id")?
            .unwrap_or_else(|| DEFAULT_LEASE_OWNER_ID.to_owned());
        let lease_ttl_ms = optional_u64(object, "lease_ttl_ms")?.unwrap_or_default();
        let context = object
            .get("context")
            .map_or_else(|| Ok(CeremonyContext::empty()), context_from_json)?;

        Ok(Self {
            definition_yaml,
            ceremony_id: CeremonyId::new(ceremony_id).map_err(|error| error.to_string())?,
            context,
            lease_owner_id: LeaseOwnerId::new(lease_owner_id).map_err(|error| error.to_string())?,
            lease_ttl: DurationMs::from_millis(if lease_ttl_ms == 0 {
                DEFAULT_LEASE_TTL_MS
            } else {
                lease_ttl_ms
            }),
        })
    }
}

fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, String> {
    optional_string(object, field)?.ok_or_else(|| format!("missing required field `{field}`"))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>, String> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("field `{field}` must be a string"))?
        .trim();
    if value.is_empty() {
        return Err(format!("field `{field}` must not be blank"));
    }
    Ok(Some(value.to_owned()))
}

fn optional_u64(object: &Map<String, Value>, field: &str) -> Result<Option<u64>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| format!("field `{field}` must be a non-negative integer"))
        })
        .transpose()
}

fn context_from_json(value: &Value) -> Result<CeremonyContext, String> {
    serde_json::from_value(value.clone()).map_err(|error| format!("invalid `context`: {error}"))
}
