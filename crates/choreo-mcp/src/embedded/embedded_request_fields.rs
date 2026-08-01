use choreo_core::value_objects::{Attributes, AuditActorKind, CeremonyContext, CeremonyId, RoleId};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::{Map, Value};

pub(super) fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, String> {
    optional_string(object, field)?.ok_or_else(|| format!("missing required field `{field}`"))
}

pub(super) fn required_strings(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Vec<String>, String> {
    let value = object
        .get(field)
        .ok_or_else(|| format!("missing required field `{field}`"))?;
    let values = value
        .as_array()
        .ok_or_else(|| format!("field `{field}` must be an array of strings"))?;
    values
        .iter()
        .map(|value| {
            let value = value
                .as_str()
                .ok_or_else(|| format!("field `{field}` must contain only strings"))?;
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(format!("field `{field}` must not contain blank strings"));
            }
            Ok(trimmed.to_owned())
        })
        .collect()
}

pub(super) fn optional_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<String>, String> {
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

pub(super) fn optional_u64(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<u64>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| format!("field `{field}` must be a non-negative integer"))
        })
        .transpose()
}

pub(super) fn optional_attributes(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Attributes, String> {
    object.get(field).map_or_else(
        || Ok(Attributes::empty()),
        |value| {
            serde_json::from_value(value.clone())
                .map_err(|error| format!("invalid `{field}`: {error}"))
        },
    )
}

pub(super) fn optional_role_ids(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<Vec<RoleId>>, String> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("field `{field}` must be an array of strings"))?;
    let role_ids = values
        .iter()
        .map(|value| {
            let raw = value
                .as_str()
                .ok_or_else(|| format!("field `{field}` must contain only strings"))?;
            RoleId::new(raw).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(role_ids))
}

pub(super) fn context_from_json(value: &Value) -> Result<CeremonyContext, String> {
    serde_json::from_value(value.clone()).map_err(|error| format!("invalid `context`: {error}"))
}

pub(super) async fn load_instance_definition(
    choreographer: &EmbeddedChoreographer,
    ceremony_id: &CeremonyId,
) -> Result<
    (
        choreo_core::entities::CeremonyDefinition,
        choreo_core::entities::CeremonyInstance,
    ),
    String,
> {
    let instance = choreographer
        .instance(ceremony_id)
        .await
        .map_err(|error| format!("failed to load ceremony instance: {error}"))?;
    let definition = choreographer
        .definition_for(&instance)
        .await
        .map_err(|error| format!("failed to load ceremony definition: {error}"))?;
    Ok((definition, instance))
}

/// What kind of party the caller says acted.
///
/// Refused rather than defaulted: a default would put a kind in the
/// journal that nobody chose, and the reason the field exists at all is
/// that the engine must not choose one.
///
/// One parser for every verb that asks, in the module the verbs already
/// share. Kept per-file, it is how one tool quietly starts accepting a
/// spelling another refuses.
pub(super) fn required_actor_kind(
    object: &Map<String, Value>,
    field: &str,
) -> Result<AuditActorKind, String> {
    Ok(match required_string(object, field)?.as_str() {
        "human" => AuditActorKind::Human,
        "agent" => AuditActorKind::Agent,
        "service" => AuditActorKind::Service,
        "engine" => AuditActorKind::Engine,
        other => {
            return Err(format!(
                "`{field}` must be human, agent, service or engine, not {other}"
            ))
        }
    })
}
