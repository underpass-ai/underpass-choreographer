//! Fixture-backed [`ChoreoMcpToolBackend`].
//!
//! Returns canned, deterministic responses for every tool. Useful for:
//!
//! - validating client wiring (Claude Desktop / Codex CLI) without a
//!   running choreographer in reach;
//! - smoke tests in repos that consume this crate;
//! - documenting the expected response shape — the fixtures are the
//!   reference JSON that real gRPC responses will mirror field-for-field.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::backend::{ChoreoMcpToolBackend, ChoreoMcpToolFuture};

/// Backend that returns canned JSON for every tool. The shapes are
/// kept aligned with what [`crate::grpc::GrpcChoreoMcpBackend`] will
/// emit in live mode so client wiring is identical across modes.
#[derive(Debug, Default, Clone)]
pub struct FixtureChoreoMcpBackend;

#[async_trait]
impl ChoreoMcpToolBackend for FixtureChoreoMcpBackend {
    fn backend_name(&self) -> &'static str {
        "fixture"
    }

    fn supports_tool(&self, name: &str) -> bool {
        crate::protocol::is_grpc_tool(name)
    }

    fn call_tool<'a>(&'a self, name: &'a str, _arguments: &'a Value) -> ChoreoMcpToolFuture<'a> {
        Box::pin(async move {
            let structured = match name {
                "choreo_deliberate" => deliberate_fixture(),
                "choreo_stream_deliberation" => stream_fixture(),
                "choreo_get_deliberation_result" => get_deliberation_fixture(),
                "choreo_orchestrate" => orchestrate_fixture(),
                "choreo_create_council" => create_council_fixture(),
                "choreo_list_councils" => list_councils_fixture(),
                "choreo_delete_council" => delete_council_fixture(),
                "choreo_register_agent" => register_agent_fixture(),
                "choreo_unregister_agent" => unregister_agent_fixture(),
                "choreo_process_trigger_event" => process_trigger_fixture(),
                "choreo_run_council_decision" => run_council_decision_fixture(),
                "choreo_register_contract" => register_contract_fixture(),
                "choreo_list_contracts" => list_contracts_fixture(),
                "choreo_delete_contract" => delete_contract_fixture(),
                "choreo_run_ceremony" => run_ceremony_fixture(),
                "choreo_get_status" => get_status_fixture(),
                "choreo_get_metrics" => get_metrics_fixture(),
                other => {
                    return Err(format!(
                        "fixture backend: unknown tool `{other}` (this is a client-side typo, not a backend error)"
                    ));
                }
            };
            Ok(crate::protocol::tool_success_result(structured))
        })
    }
}

// ---------------------------------------------------------------------------
// Canned fixtures. Stable across releases so client wiring stays
// reproducible. New fields appear here first, then in the real
// adapter; tests in `tests/stdio_protocol.rs` pin the shape.
// ---------------------------------------------------------------------------

fn deliberate_fixture() -> Value {
    json!({
        "task_id": "task-fixture-1",
        "winner_proposal_id": "proposal-fixture-a",
        "duration_ms": 42,
        "results": [
            {
                "rank": 0,
                "proposal": {
                    "proposal_id": "proposal-fixture-a",
                    "author_agent_id": "agent-fixture-1",
                    "content": "fixture answer",
                    "metadata": {},
                    "revision_count": 0
                },
                "validation": {
                    "score": 1.0,
                    "reports": [
                        { "kind": "content-non-empty", "passed": true, "summary": "ok", "details": {} }
                    ]
                }
            }
        ],
        "metadata": { "fixture": true }
    })
}

fn stream_fixture() -> Value {
    json!({
        "task_id": "task-fixture-1",
        "frames": [
            { "phase": "DELIBERATION_PHASE_PROPOSING", "emitted_at": null, "payload": null },
            { "phase": "DELIBERATION_PHASE_REVISING", "emitted_at": null, "payload": null },
            { "phase": "DELIBERATION_PHASE_VALIDATING", "emitted_at": null, "payload": null },
            { "phase": "DELIBERATION_PHASE_SCORING", "emitted_at": null, "payload": null },
            {
                "phase": "DELIBERATION_PHASE_COMPLETED",
                "emitted_at": null,
                "payload": { "kind": "result", "result": deliberate_fixture()["results"][0].clone() }
            }
        ],
        "winner": deliberate_fixture()["results"][0].clone()
    })
}

fn get_deliberation_fixture() -> Value {
    json!({
        "found": true,
        "result": deliberate_fixture()
    })
}

fn orchestrate_fixture() -> Value {
    json!({
        "task_id": "task-fixture-1",
        "execution_id": "exec-fixture-1",
        "duration_ms": 73,
        "winner": deliberate_fixture()["results"][0].clone(),
        "candidates": [],
        "metadata": { "fixture": true }
    })
}

fn create_council_fixture() -> Value {
    json!({
        "council": {
            "specialty": "triage",
            "num_agents": 1,
            "created_at": null,
            "agents": []
        }
    })
}

fn list_councils_fixture() -> Value {
    json!({
        "councils": [
            { "specialty": "triage", "num_agents": 1, "created_at": null, "agents": [] }
        ]
    })
}

fn delete_council_fixture() -> Value {
    json!({ "deleted": true })
}

fn register_agent_fixture() -> Value {
    json!({ "agent_id": "agent-fixture-1" })
}

fn unregister_agent_fixture() -> Value {
    json!({ "unregistered": true })
}

fn process_trigger_fixture() -> Value {
    json!({
        "ack": {
            "event_id": "evt-fixture-1",
            "accepted": true,
            "dispatched_task_ids": ["task-fixture-1"],
            "reason": ""
        }
    })
}

fn get_status_fixture() -> Value {
    json!({
        "version": "fixture",
        "uptime_seconds": 0,
        "health": "healthy",
        "stats": null
    })
}

fn get_metrics_fixture() -> Value {
    json!({
        "stats": {
            "total_deliberations": 0,
            "total_orchestrations": 0,
            "total_duration_ms": 0,
            "average_duration_ms": 0.0,
            "per_specialty_counts": {}
        }
    })
}

fn run_council_decision_fixture() -> Value {
    json!({
        "task_id": "task-fixture-1",
        "winner": deliberate_fixture()["results"][0].clone(),
        "validation": {
            "passed": true,
            "candidates_passed": 1,
            "candidates_total": 1
        },
        "candidates": [
            {
                "proposal_id": "proposal-fixture-a",
                "author_agent_id": "agent-fixture-1",
                "score": 1.0,
                "reports": [
                    { "kind": "content-non-empty", "passed": true, "summary": "ok", "details": {} }
                ],
                "rank": 0,
                "passed": true,
                "revision_count": 0
            }
        ],
        "duration_ms": 42,
        "validation_mode": "VALIDATION_MODE_STRICT"
    })
}

fn register_contract_fixture() -> Value {
    json!({ "contract_id": "contract-fixture-1" })
}

fn list_contracts_fixture() -> Value {
    json!({
        "contracts": [
            {
                "contract_id": "contract-fixture-1",
                "format": "json_object",
                "fields": {},
                "json_schema": ""
            }
        ]
    })
}

fn delete_contract_fixture() -> Value {
    json!({ "deleted": true })
}

fn run_ceremony_fixture() -> Value {
    json!({
        "ceremony_id": "ceremony-fixture-1",
        "definition_name": "fixture_ceremony",
        "definition_version": "1.0",
        "final_state": "CLOSED",
        "completed": true,
        "steps": [
            {
                "state_id": "OPEN",
                "step_id": "collect_context",
                "role_id": "FACILITATOR",
                "status": "COMPLETED",
                "attempt": 1,
                "output": "context collected: the team agreed on the brief"
            }
        ],
        "mermaid_sequence": "sequenceDiagram\n    FACILITATOR->>FACILITATOR: collect_context [noop_step]"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn unknown_tool_returns_explicit_error() {
        let backend = FixtureChoreoMcpBackend;
        let err = backend.call_tool("nope", &json!({})).await.unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[tokio::test]
    async fn deliberate_returns_mcp_success_envelope() {
        let backend = FixtureChoreoMcpBackend;
        let v = backend
            .call_tool("choreo_deliberate", &json!({}))
            .await
            .unwrap();
        assert_eq!(v["isError"], false);
        assert_eq!(
            v["structuredContent"]["winner_proposal_id"],
            "proposal-fixture-a"
        );
    }

    #[tokio::test]
    async fn every_tool_has_a_fixture() {
        let backend = FixtureChoreoMcpBackend;
        let catalog = crate::protocol::tools_list_result(|name| backend.supports_tool(name));
        for tool in catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
        {
            let v = backend.call_tool(tool, &json!({})).await.unwrap();
            assert_eq!(v["isError"], false, "{tool} fixture must be a success");
        }
    }
}
