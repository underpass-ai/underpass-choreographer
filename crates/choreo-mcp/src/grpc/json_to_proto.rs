//! JSON-arguments → proto-request mappers.
//!
//! Hand-written field-by-field so every choreo.proto field has an
//! explicit code path. No `serde_json::to_value(proto)` shortcuts —
//! that would happily ignore fields the schema added and silently
//! lose information.

use std::collections::BTreeMap;

use choreo_mcp_proto::v1 as pb;
use prost_types::{
    value::Kind as PbKind, ListValue, Struct as PbStruct, Timestamp, Value as PbValue,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Primitive helpers
// ---------------------------------------------------------------------------

pub(crate) fn require_object<'a>(
    value: &'a Value,
    where_: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{where_}: expected an object"))
}

pub(crate) fn require_str<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, String> {
    obj.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing required string `{key}`"))
}

pub(crate) fn optional_str<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

pub(crate) fn optional_u32(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u32, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| format!("`{key}` must be a non-negative u32")),
        Some(_) => Err(format!("`{key}` must be a number")),
    }
}

pub(crate) fn optional_u64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u64, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n
            .as_u64()
            .ok_or_else(|| format!("`{key}` must be a non-negative u64")),
        Some(_) => Err(format!("`{key}` must be a number")),
    }
}

pub(crate) fn optional_bool(obj: &serde_json::Map<String, Value>, key: &str) -> bool {
    matches!(obj.get(key), Some(Value::Bool(true)))
}

pub(crate) fn json_to_pb_value(value: &Value) -> PbValue {
    let kind = match value {
        Value::Null => PbKind::NullValue(0),
        Value::Bool(b) => PbKind::BoolValue(*b),
        Value::Number(n) => PbKind::NumberValue(n.as_f64().unwrap_or_default()),
        Value::String(s) => PbKind::StringValue(s.clone()),
        Value::Array(arr) => PbKind::ListValue(ListValue {
            values: arr.iter().map(json_to_pb_value).collect(),
        }),
        Value::Object(obj) => PbKind::StructValue(json_object_to_pb_struct(obj)),
    };
    PbValue { kind: Some(kind) }
}

pub(crate) fn json_object_to_pb_struct(obj: &serde_json::Map<String, Value>) -> PbStruct {
    PbStruct {
        fields: obj
            .iter()
            .map(|(k, v)| (k.clone(), json_to_pb_value(v)))
            .collect(),
    }
}

pub(crate) fn optional_pb_struct(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<PbStruct>, String> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => Ok(Some(json_object_to_pb_struct(o))),
        Some(_) => Err(format!("`{key}` must be a JSON object")),
    }
}

/// A JSON array of strings. Anything that is not one is absent, not
/// an error: proto has no way to say "this was the wrong type", and a
/// caller who sent the wrong thing is better served by the engine
/// refusing the call than by a parse error about a list.
pub(crate) fn string_array(obj: &serde_json::Map<String, Value>, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_rfc3339_to_timestamp(value: &str) -> Result<Timestamp, String> {
    // Minimal RFC3339 parsing using `time` crate.
    let dt = time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map_err(|err| format!("invalid RFC3339 timestamp `{value}`: {err}"))?;
    let nanos = dt.unix_timestamp_nanos();
    let seconds = (nanos / 1_000_000_000) as i64;
    let nanos = (nanos % 1_000_000_000) as i32;
    Ok(Timestamp { seconds, nanos })
}

pub(crate) fn optional_timestamp(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Timestamp>, String> {
    match optional_str(obj, key) {
        None | Some("") => Ok(None),
        Some(value) => parse_rfc3339_to_timestamp(value).map(Some),
    }
}

// ---------------------------------------------------------------------------
// Composite mappers (one per proto type)
// ---------------------------------------------------------------------------

pub(crate) fn task_from_json(value: &Value) -> Result<pb::Task, String> {
    let obj = require_object(value, "task")?;
    Ok(pb::Task {
        task_id: require_str(obj, "task_id")?.to_string(),
        description: optional_str(obj, "description").unwrap_or("").to_string(),
        specialty: require_str(obj, "specialty")?.to_string(),
        constraints: optional_constraints(obj.get("constraints"))?,
        attributes: optional_pb_struct(obj, "attributes")?,
        external_context: optional_external_context(obj.get("external_context"))?,
        metadata: optional_task_metadata(obj.get("metadata"))?,
    })
}

fn optional_constraints(value: Option<&Value>) -> Result<Option<pb::Constraints>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let obj = require_object(value, "task.constraints")?;
    Ok(Some(pb::Constraints {
        rubric: optional_pb_struct(obj, "rubric")?,
        rounds: optional_u32(obj, "rounds")?,
        num_agents: optional_u32(obj, "num_agents")?,
        deadline_ms: optional_u64(obj, "deadline_ms")?,
        output_contract: optional_output_contract(obj.get("output_contract"))?,
    }))
}

fn optional_output_contract(value: Option<&Value>) -> Result<Option<pb::OutputContract>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(output_contract_from_json(value)?))
}

pub(crate) fn output_contract_from_json(value: &Value) -> Result<pb::OutputContract, String> {
    let obj = require_object(value, "output_contract")?;
    let format = match optional_str(obj, "format") {
        Some("json_object") | None => pb::OutputFormat::JsonObject as i32,
        Some(other) => {
            return Err(format!(
                "output_contract.format `{other}` is not supported; only `json_object` is implemented"
            ));
        }
    };
    let mut fields: BTreeMap<String, pb::OutputFieldRule> = BTreeMap::new();
    if let Some(map) = obj.get("fields").and_then(Value::as_object) {
        for (name, rule_value) in map {
            let rule_obj = require_object(rule_value, "output_contract.fields.<rule>")?;
            let required = optional_bool(rule_obj, "required");
            let allowed = rule_obj
                .get("allowed_string_values")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            fields.insert(
                name.clone(),
                pb::OutputFieldRule {
                    required,
                    allowed_string_values: allowed,
                },
            );
        }
    }
    let json_schema = optional_str(obj, "json_schema").unwrap_or("").to_string();
    Ok(pb::OutputContract {
        contract_id: optional_str(obj, "contract_id").unwrap_or("").to_string(),
        format,
        fields: fields.into_iter().collect(),
        json_schema,
    })
}

pub(crate) fn run_council_decision_request_from_json(
    value: &Value,
) -> Result<pb::RunCouncilDecisionRequest, String> {
    let obj = require_object(value, "tools/call.arguments")?;
    let contract_id = require_str(obj, "contract_id")?.to_string();
    let description = require_str(obj, "description")?.to_string();

    let council_id = optional_str(obj, "council_id").map(str::to_string);
    let specialty = optional_str(obj, "specialty").map(str::to_string);
    let selector = match (council_id, specialty) {
        (Some(_), Some(_)) => {
            return Err("exactly one of `council_id` or `specialty` must be set, not both".into());
        }
        (None, None) => {
            return Err("missing required selector: set `council_id` or `specialty`".into());
        }
        (Some(id), None) => Some(pb::run_council_decision_request::Selector::CouncilId(id)),
        (None, Some(s)) => Some(pb::run_council_decision_request::Selector::Specialty(s)),
    };

    let validation_mode = match optional_str(obj, "validation_mode") {
        None | Some("" | "VALIDATION_MODE_UNSPECIFIED") => pb::ValidationMode::Unspecified as i32,
        Some("VALIDATION_MODE_STRICT") => pb::ValidationMode::Strict as i32,
        Some("VALIDATION_MODE_WARN") => pb::ValidationMode::Warn as i32,
        Some(other) => return Err(format!("unknown validation_mode `{other}`")),
    };

    Ok(pb::RunCouncilDecisionRequest {
        contract_id,
        external_context: optional_external_context(obj.get("external_context"))?,
        validation_mode,
        metadata: optional_task_metadata(obj.get("metadata"))?,
        description,
        selector,
    })
}

pub(crate) fn run_ceremony_request_from_json(
    value: &Value,
) -> Result<pb::RunCeremonyRequest, String> {
    let obj = require_object(value, "tools/call.arguments")?;
    Ok(pb::RunCeremonyRequest {
        actor_id: require_str(obj, "actor_id")?.to_string(),
        actor_kind: require_str(obj, "actor_kind")?.to_string(),
        ceremony_id: optional_str(obj, "ceremony_id").unwrap_or("").to_string(),
        definition_yaml: require_str(obj, "definition_yaml")?.to_string(),
        context: optional_pb_struct(obj, "context")?,
        lease_owner_id: optional_str(obj, "lease_owner_id")
            .unwrap_or("")
            .to_string(),
        lease_ttl_ms: optional_u64(obj, "lease_ttl_ms")?,
    })
}

fn optional_external_context(
    value: Option<&Value>,
) -> Result<Option<pb::ExternalContextBundle>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let obj = require_object(value, "external_context")?;
    let items = obj
        .get("items")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(context_item_from_json)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let references = obj
        .get("references")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(context_reference_from_json)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Some(pb::ExternalContextBundle {
        bundle_id: optional_str(obj, "bundle_id").unwrap_or("").to_string(),
        schema_version: optional_str(obj, "schema_version")
            .unwrap_or("")
            .to_string(),
        summary: optional_context_summary(obj.get("summary"))?,
        items,
        references,
        metadata: optional_pb_struct(obj, "metadata")?,
    }))
}

fn optional_context_summary(value: Option<&Value>) -> Result<Option<pb::ContextSummary>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let obj = require_object(value, "external_context.summary")?;
    Ok(Some(pb::ContextSummary {
        text: optional_str(obj, "text").unwrap_or("").to_string(),
        attributes: optional_pb_struct(obj, "attributes")?,
    }))
}

fn context_item_from_json(value: &Value) -> Result<pb::ContextItem, String> {
    let obj = require_object(value, "external_context.items[]")?;
    let reference_ids = obj
        .get("reference_ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(pb::ContextItem {
        item_id: require_str(obj, "item_id")?.to_string(),
        kind: require_str(obj, "kind")?.to_string(),
        title: optional_str(obj, "title").unwrap_or("").to_string(),
        narrative: optional_str(obj, "narrative").unwrap_or("").to_string(),
        attributes: optional_pb_struct(obj, "attributes")?,
        reference_ids,
    })
}

fn context_reference_from_json(value: &Value) -> Result<pb::ContextReference, String> {
    let obj = require_object(value, "external_context.references[]")?;
    Ok(pb::ContextReference {
        reference_id: require_str(obj, "reference_id")?.to_string(),
        uri: require_str(obj, "uri")?.to_string(),
        title: optional_str(obj, "title").unwrap_or("").to_string(),
        media_type: optional_str(obj, "media_type").unwrap_or("").to_string(),
        attributes: optional_pb_struct(obj, "attributes")?,
    })
}

fn optional_task_metadata(value: Option<&Value>) -> Result<Option<pb::TaskMetadata>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.is_null() {
        return Ok(None);
    }
    let obj = require_object(value, "task.metadata")?;
    Ok(Some(pb::TaskMetadata {
        source_event_id: optional_str(obj, "source_event_id")
            .unwrap_or("")
            .to_string(),
        causation_id: optional_str(obj, "causation_id").unwrap_or("").to_string(),
        correlation_id: optional_str(obj, "correlation_id")
            .unwrap_or("")
            .to_string(),
        council_contract_id: optional_str(obj, "council_contract_id")
            .unwrap_or("")
            .to_string(),
        output_contract_id: optional_str(obj, "output_contract_id")
            .unwrap_or("")
            .to_string(),
        execution_profile: optional_pb_struct(obj, "execution_profile")?,
    }))
}

pub(crate) fn agent_summary_from_json(value: &Value) -> Result<pb::AgentSummary, String> {
    let obj = require_object(value, "agent")?;
    Ok(pb::AgentSummary {
        agent_id: require_str(obj, "agent_id")?.to_string(),
        specialty: require_str(obj, "specialty")?.to_string(),
        kind: require_str(obj, "kind")?.to_string(),
        attributes: optional_pb_struct(obj, "attributes")?,
    })
}

pub(crate) fn trigger_event_from_json(value: &Value) -> Result<pb::TriggerEvent, String> {
    let obj = require_object(value, "event")?;
    let requested_specialties = obj
        .get("requested_specialties")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if requested_specialties.is_empty() {
        return Err("event.requested_specialties must be a non-empty array of strings".into());
    }

    let constraints = optional_constraints(obj.get("constraints"))?;
    let external_context = optional_external_context(obj.get("external_context"))?;

    Ok(pb::TriggerEvent {
        event_id: require_str(obj, "event_id")?.to_string(),
        kind: require_str(obj, "kind")?.to_string(),
        source: require_str(obj, "source")?.to_string(),
        emitted_at: optional_timestamp(obj, "emitted_at")?,
        requested_specialties,
        task_description_template: optional_str(obj, "task_description_template")
            .unwrap_or("")
            .to_string(),
        constraints,
        payload: optional_pb_struct(obj, "payload")?,
        external_context,
        correlation_id: optional_str(obj, "correlation_id")
            .unwrap_or("")
            .to_string(),
        causation_id: optional_str(obj, "causation_id").unwrap_or("").to_string(),
    })
}
