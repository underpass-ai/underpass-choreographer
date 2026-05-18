//! proto-response → JSON mappers.
//!
//! Mirror of `json_to_proto.rs`. Every proto field gets a named JSON
//! key explicitly so the wire schema is documented in the code and
//! reviewers can spot accidental drops at PR time.

use choreo_mcp_proto::v1 as pb;
use prost_types::{
    value::Kind as PbKind, ListValue, Struct as PbStruct, Timestamp, Value as PbValue,
};
use serde_json::{json, Map, Number as JsonNumber, Value};

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

pub(crate) fn pb_value_to_json(value: PbValue) -> Value {
    match value.kind {
        None | Some(PbKind::NullValue(_)) => Value::Null,
        Some(PbKind::BoolValue(b)) => Value::Bool(b),
        Some(PbKind::NumberValue(n)) => JsonNumber::from_f64(n).map_or(Value::Null, Value::Number),
        Some(PbKind::StringValue(s)) => Value::String(s),
        Some(PbKind::ListValue(ListValue { values })) => {
            Value::Array(values.into_iter().map(pb_value_to_json).collect())
        }
        Some(PbKind::StructValue(s)) => Value::Object(pb_struct_to_json(s)),
    }
}

pub(crate) fn pb_struct_to_json(s: PbStruct) -> Map<String, Value> {
    s.fields
        .into_iter()
        .map(|(k, v)| (k, pb_value_to_json(v)))
        .collect()
}

pub(crate) fn optional_pb_struct_to_json(s: Option<PbStruct>) -> Value {
    match s {
        Some(s) => Value::Object(pb_struct_to_json(s)),
        None => Value::Object(Map::new()),
    }
}

pub(crate) fn timestamp_to_rfc3339(ts: Option<&Timestamp>) -> Value {
    let Some(Timestamp { seconds, nanos }) = ts else {
        return Value::Null;
    };
    let nanos_total = i128::from(*seconds) * 1_000_000_000 + i128::from(*nanos);
    time::OffsetDateTime::from_unix_timestamp_nanos(nanos_total)
        .ok()
        .and_then(|dt| {
            dt.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .map_or(Value::Null, Value::String)
}

fn phase_name(phase: i32) -> &'static str {
    match pb::DeliberationPhase::try_from(phase).unwrap_or(pb::DeliberationPhase::Unspecified) {
        pb::DeliberationPhase::Unspecified => "DELIBERATION_PHASE_UNSPECIFIED",
        pb::DeliberationPhase::Proposing => "DELIBERATION_PHASE_PROPOSING",
        pb::DeliberationPhase::Revising => "DELIBERATION_PHASE_REVISING",
        pb::DeliberationPhase::Validating => "DELIBERATION_PHASE_VALIDATING",
        pb::DeliberationPhase::Scoring => "DELIBERATION_PHASE_SCORING",
        pb::DeliberationPhase::Completed => "DELIBERATION_PHASE_COMPLETED",
    }
}

// ---------------------------------------------------------------------------
// Composite responses
// ---------------------------------------------------------------------------

pub(crate) fn proposal_to_json(p: pb::Proposal) -> Value {
    json!({
        "proposal_id": p.proposal_id,
        "author_agent_id": p.author_agent_id,
        "content": p.content,
        "metadata": optional_pb_struct_to_json(p.metadata),
    })
}

pub(crate) fn validation_outcome_to_json(v: pb::ValidationOutcome) -> Value {
    let reports: Vec<Value> = v
        .reports
        .into_iter()
        .map(|r| {
            json!({
                "kind": r.kind,
                "passed": r.passed,
                "summary": r.summary,
                "details": optional_pb_struct_to_json(r.details),
            })
        })
        .collect();
    let passed_overall = reports
        .iter()
        .all(|r| r["passed"].as_bool().unwrap_or(false));
    json!({
        "score": v.score,
        "passed": passed_overall,
        "reports": reports,
    })
}

pub(crate) fn deliberation_result_to_json(r: pb::DeliberationResult) -> Value {
    json!({
        "rank": r.rank,
        "proposal": r.proposal.map_or(Value::Null, proposal_to_json),
        "validation": r
            .validation
            .map_or(Value::Null, validation_outcome_to_json),
    })
}

pub(crate) fn deliberate_response_to_json(r: pb::DeliberateResponse) -> Value {
    json!({
        "task_id": r.task_id,
        "winner_proposal_id": r.winner_proposal_id,
        "duration_ms": r.duration_ms,
        "results": r
            .results
            .into_iter()
            .map(deliberation_result_to_json)
            .collect::<Vec<_>>(),
        "metadata": optional_pb_struct_to_json(r.metadata),
    })
}

pub(crate) fn deliberation_update_to_json(u: pb::DeliberationUpdate) -> Value {
    let payload = match u.payload {
        None => Value::Null,
        Some(pb::deliberation_update::Payload::Proposal(p)) => json!({
            "kind": "proposal",
            "proposal": proposal_to_json(p),
        }),
        Some(pb::deliberation_update::Payload::Critique(c)) => json!({
            "kind": "critique",
            "critique": {
                "reviewer_agent_id": c.reviewer_agent_id,
                "target_proposal_id": c.target_proposal_id,
                "feedback": c.feedback,
            }
        }),
        Some(pb::deliberation_update::Payload::Revision(r)) => json!({
            "kind": "revision",
            "revision": {
                "author_agent_id": r.author_agent_id,
                "proposal_id": r.proposal_id,
                "updated_content": r.updated_content,
            }
        }),
        Some(pb::deliberation_update::Payload::Validation(v)) => json!({
            "kind": "validation",
            "validation": validation_outcome_to_json(v),
        }),
        Some(pb::deliberation_update::Payload::Result(r)) => json!({
            "kind": "result",
            "result": deliberation_result_to_json(r),
        }),
    };
    json!({
        "task_id": u.task_id,
        "phase": phase_name(u.phase),
        "emitted_at": timestamp_to_rfc3339(u.emitted_at.as_ref()),
        "payload": payload,
    })
}

pub(crate) fn orchestrate_response_to_json(r: pb::OrchestrateResponse) -> Value {
    json!({
        "task_id": r.task_id,
        "execution_id": r.execution_id,
        "duration_ms": r.duration_ms,
        "winner": r
            .winner
            .map_or(Value::Null, deliberation_result_to_json),
        "candidates": r
            .candidates
            .into_iter()
            .map(deliberation_result_to_json)
            .collect::<Vec<_>>(),
        "metadata": optional_pb_struct_to_json(r.metadata),
    })
}

pub(crate) fn agent_summary_to_json(a: pb::AgentSummary) -> Value {
    json!({
        "agent_id": a.agent_id,
        "specialty": a.specialty,
        "kind": a.kind,
        "attributes": optional_pb_struct_to_json(a.attributes),
    })
}

pub(crate) fn council_summary_to_json(c: pb::CouncilSummary) -> Value {
    json!({
        "specialty": c.specialty,
        "num_agents": c.num_agents,
        "created_at": timestamp_to_rfc3339(c.created_at.as_ref()),
        "agents": c
            .agents
            .into_iter()
            .map(agent_summary_to_json)
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn trigger_ack_to_json(a: &pb::TriggerAck) -> Value {
    json!({
        "event_id": a.event_id,
        "accepted": a.accepted,
        "dispatched_task_ids": a.dispatched_task_ids,
        "reason": a.reason,
    })
}

pub(crate) fn output_contract_to_json(c: pb::OutputContract) -> Value {
    let format = match pb::OutputFormat::try_from(c.format).unwrap_or(pb::OutputFormat::Unspecified)
    {
        pb::OutputFormat::Unspecified | pb::OutputFormat::JsonObject => "json_object",
    };
    let fields: Map<String, Value> = c
        .fields
        .into_iter()
        .map(|(name, rule)| {
            (
                name,
                json!({
                    "required": rule.required,
                    "allowed_string_values": rule.allowed_string_values,
                }),
            )
        })
        .collect();
    json!({
        "contract_id": c.contract_id,
        "format": format,
        "fields": Value::Object(fields),
        "json_schema": c.json_schema,
    })
}

fn validation_mode_name(value: i32) -> &'static str {
    match pb::ValidationMode::try_from(value).unwrap_or(pb::ValidationMode::Unspecified) {
        pb::ValidationMode::Unspecified => "VALIDATION_MODE_UNSPECIFIED",
        pb::ValidationMode::Strict => "VALIDATION_MODE_STRICT",
        pb::ValidationMode::Warn => "VALIDATION_MODE_WARN",
    }
}

fn candidate_summary_to_json(c: pb::CandidateSummary) -> Value {
    json!({
        "proposal_id": c.proposal_id,
        "author_agent_id": c.author_agent_id,
        "score": c.score,
        "reports": c
            .reports
            .into_iter()
            .map(|r| json!({
                "kind": r.kind,
                "passed": r.passed,
                "summary": r.summary,
                "details": optional_pb_struct_to_json(r.details),
            }))
            .collect::<Vec<_>>(),
        "rank": c.rank,
        "passed": c.passed,
    })
}

fn validation_outcome_summary_to_json(v: pb::ValidationOutcomeSummary) -> Value {
    json!({
        "passed": v.passed,
        "candidates_passed": v.candidates_passed,
        "candidates_total": v.candidates_total,
    })
}

pub(crate) fn run_council_decision_response_to_json(r: pb::RunCouncilDecisionResponse) -> Value {
    json!({
        "task_id": r.task_id,
        "winner": r.winner.map_or(Value::Null, deliberation_result_to_json),
        "validation": r
            .validation
            .map_or(Value::Null, validation_outcome_summary_to_json),
        "candidates": r
            .candidates
            .into_iter()
            .map(candidate_summary_to_json)
            .collect::<Vec<_>>(),
        "duration_ms": r.duration_ms,
        "validation_mode": validation_mode_name(r.validation_mode),
    })
}

pub(crate) fn statistics_to_json(s: pb::Statistics) -> Value {
    let per_specialty: Map<String, Value> = s
        .per_specialty_counts
        .into_iter()
        .map(|(k, v)| (k, Value::Number(JsonNumber::from(v))))
        .collect();
    json!({
        "total_deliberations": s.total_deliberations,
        "total_orchestrations": s.total_orchestrations,
        "total_duration_ms": s.total_duration_ms,
        "average_duration_ms": s.average_duration_ms,
        "per_specialty_counts": Value::Object(per_specialty),
    })
}
