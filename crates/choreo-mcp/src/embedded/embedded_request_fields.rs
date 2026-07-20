use choreo_core::value_objects::{Attributes, CeremonyContext, CeremonyId, RoleId};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::{Map, Value};

pub(super) fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, String> {
    optional_string(object, field)?.ok_or_else(|| format!("missing required field `{field}`"))
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
        .definition(instance.definition_name(), instance.definition_version())
        .await
        .map_err(|error| format!("failed to load ceremony definition: {error}"))?;
    Ok((definition, instance))
}
