//! Operator tool: run a ceremony YAML against a deployed Choreographer.
//!
//! Reads a ceremony definition from a file and drives it through the
//! `RunCeremony` RPC, printing the terminal state, the per-step role and
//! status, and the rendered Mermaid conversation diagram. Useful for
//! exercising a ceremony (including provider-backed ones) against a live
//! deployment.
//!
//! Env:
//! - `CEREMONY_YAML_PATH` (required) — path to the ceremony YAML.
//! - `CHOREOGRAPHER_ENDPOINT` (default `http://localhost:50055`).
//! - `CEREMONY_ID` (default `operator-ceremony`).
//! - `CEREMONY_LEASE_TTL_MS` (default `120000`).
//! - `CEREMONY_CONTEXT_JSON` (optional) — a JSON object passed as the
//!   ceremony context (its inputs, e.g. `{"mission_brief": "..."}`),
//!   so a generic ceremony can be driven for any mission.

use anyhow::{Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use prost_types::{value::Kind, ListValue, Struct, Value as PbValue};
use serde_json::Value as JsonValue;

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint = std::env::var("CHOREOGRAPHER_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:50055".to_owned());
    let path = std::env::var("CEREMONY_YAML_PATH").context("CEREMONY_YAML_PATH is required")?;
    let ceremony_id =
        std::env::var("CEREMONY_ID").unwrap_or_else(|_| "operator-ceremony".to_owned());
    let lease_ttl_ms = std::env::var("CEREMONY_LEASE_TTL_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(120_000);

    let definition_yaml =
        std::fs::read_to_string(&path).with_context(|| format!("reading ceremony YAML {path}"))?;
    let context = context_from_env()?;

    let mut client = ChoreographerServiceClient::connect(endpoint.clone())
        .await
        .with_context(|| format!("connecting to {endpoint}"))?;

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: ceremony_id.clone(),
            definition_yaml,
            context,
            lease_owner_id: "operator".to_owned(),
            lease_ttl_ms,
        })
        .await
        .context("RunCeremony RPC failed")?
        .into_inner();

    println!(
        "ceremony={} name={} final_state={} completed={} steps={}",
        response.ceremony_id,
        response.definition_name,
        response.final_state,
        response.completed,
        response.steps.len()
    );
    for step in &response.steps {
        println!(
            "  step={:<22} role={:<20} status={:<10} attempt={}",
            step.step_id, step.role_id, step.status, step.attempt
        );
    }
    let has_output = response.steps.iter().any(|s| !s.output.trim().is_empty());
    if has_output {
        println!("\n--- meeting record (winning contribution per step) ---");
        for step in &response.steps {
            if step.output.trim().is_empty() {
                continue;
            }
            println!("\n### {} [{}]\n{}", step.step_id, step.role_id, step.output);
        }
    }
    println!(
        "\n--- conversation diagram ---\n{}",
        response.mermaid_sequence
    );

    if !response.completed {
        anyhow::bail!(
            "ceremony did not complete; final_state={}",
            response.final_state
        );
    }
    Ok(())
}

/// Read `CEREMONY_CONTEXT_JSON` (a JSON object) into the ceremony context
/// Struct, or `None` when unset.
fn context_from_env() -> Result<Option<Struct>> {
    let Some(raw) = std::env::var("CEREMONY_CONTEXT_JSON")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let value: JsonValue =
        serde_json::from_str(&raw).context("CEREMONY_CONTEXT_JSON must be valid JSON")?;
    let object = value
        .as_object()
        .context("CEREMONY_CONTEXT_JSON must be a JSON object")?;
    Ok(Some(json_object_to_struct(object)))
}

fn json_object_to_struct(object: &serde_json::Map<String, JsonValue>) -> Struct {
    Struct {
        fields: object
            .iter()
            .map(|(key, value)| (key.clone(), json_to_pb_value(value)))
            .collect(),
    }
}

fn json_to_pb_value(value: &JsonValue) -> PbValue {
    let kind = match value {
        JsonValue::Null => Kind::NullValue(0),
        JsonValue::Bool(b) => Kind::BoolValue(*b),
        JsonValue::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        JsonValue::String(s) => Kind::StringValue(s.clone()),
        JsonValue::Array(items) => Kind::ListValue(ListValue {
            values: items.iter().map(json_to_pb_value).collect(),
        }),
        JsonValue::Object(object) => Kind::StructValue(json_object_to_struct(object)),
    };
    PbValue { kind: Some(kind) }
}
