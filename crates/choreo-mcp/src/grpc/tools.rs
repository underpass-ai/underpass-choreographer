//! Tool-name → gRPC RPC dispatch.
//!
//! One entry per choreographer RPC. The dispatcher parses JSON
//! arguments into the proto request via `json_to_proto`, calls the
//! generated tonic client, and converts the response back via
//! `proto_to_json`. tonic `Status` errors collapse to plain strings.

use choreo_mcp_proto::v1 as pb;
use choreo_mcp_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use serde_json::{json, Value};
use tonic::transport::Channel;
use uuid::Uuid;

use super::json_to_proto as j2p;
use super::proto_to_json as p2j;
use super::streaming;

/// Dispatch one tool call. Returns the **structured content** of the
/// MCP tool result (just the JSON; the caller wraps it in
/// `tool_success_result`).
#[allow(clippy::too_many_lines)] // one arm per tool; splitting fragments the dispatch table
pub(crate) async fn dispatch(
    channel: Channel,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let mut client = ChoreographerServiceClient::new(channel);

    match name {
        "choreo_deliberate" => {
            let request = build_deliberate_request(arguments)?;
            let response = client
                .deliberate(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::deliberate_response_to_json(response.into_inner()))
        }

        "choreo_stream_deliberation" => {
            let request = build_stream_deliberation_request(arguments)?;
            let response = client
                .stream_deliberation(request)
                .await
                .map_err(|s| status_error(&s))?;
            streaming::collect_stream(response.into_inner()).await
        }

        "choreo_get_deliberation_result" => {
            let request = build_get_deliberation_result_request(arguments)?;
            let response = client
                .get_deliberation_result(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::GetDeliberationResultResponse { found, result } = response.into_inner();
            Ok(json!({
                "found": found,
                "result": result.map_or(Value::Null, p2j::deliberate_response_to_json),
            }))
        }

        "choreo_orchestrate" => {
            let request = build_orchestrate_request(arguments)?;
            let response = client
                .orchestrate(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::orchestrate_response_to_json(response.into_inner()))
        }

        "choreo_create_council" => {
            let request = build_create_council_request(arguments)?;
            let response = client
                .create_council(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::CreateCouncilResponse { council } = response.into_inner();
            Ok(json!({
                "council": council.map_or(Value::Null, p2j::council_summary_to_json),
            }))
        }

        "choreo_list_councils" => {
            let request = build_list_councils_request(arguments);
            let response = client
                .list_councils(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::ListCouncilsResponse { councils } = response.into_inner();
            Ok(json!({
                "councils": councils
                    .into_iter()
                    .map(p2j::council_summary_to_json)
                    .collect::<Vec<_>>(),
            }))
        }

        "choreo_delete_council" => {
            let request = build_delete_council_request(arguments)?;
            let response = client
                .delete_council(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::DeleteCouncilResponse { deleted } = response.into_inner();
            Ok(json!({ "deleted": deleted }))
        }

        "choreo_register_agent" => {
            let request = build_register_agent_request(arguments)?;
            let response = client
                .register_agent(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::RegisterAgentResponse { agent_id } = response.into_inner();
            Ok(json!({ "agent_id": agent_id }))
        }

        "choreo_unregister_agent" => {
            let request = build_unregister_agent_request(arguments)?;
            let response = client
                .unregister_agent(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::UnregisterAgentResponse { unregistered } = response.into_inner();
            Ok(json!({ "unregistered": unregistered }))
        }

        "choreo_process_trigger_event" => {
            let request = build_process_trigger_event_request(arguments)?;
            let response = client
                .process_trigger_event(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::ProcessTriggerEventResponse { ack } = response.into_inner();
            Ok(json!({
                "ack": ack.as_ref().map_or(Value::Null, p2j::trigger_ack_to_json),
            }))
        }

        "choreo_run_council_decision" => {
            let request = build_run_council_decision_request(arguments)?;
            let response = client
                .run_council_decision(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::run_council_decision_response_to_json(
                response.into_inner(),
            ))
        }

        "choreo_register_contract" => {
            let request = build_register_contract_request(arguments)?;
            let response = client
                .register_contract(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::RegisterContractResponse { contract_id } = response.into_inner();
            Ok(json!({ "contract_id": contract_id }))
        }

        "choreo_list_contracts" => {
            let response = client
                .list_contracts(pb::ListContractsRequest {})
                .await
                .map_err(|s| status_error(&s))?;
            let pb::ListContractsResponse { contracts } = response.into_inner();
            Ok(json!({
                "contracts": contracts
                    .into_iter()
                    .map(p2j::output_contract_to_json)
                    .collect::<Vec<_>>(),
            }))
        }

        "choreo_delete_contract" => {
            let request = build_delete_contract_request(arguments)?;
            let response = client
                .delete_contract(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::DeleteContractResponse { deleted } = response.into_inner();
            Ok(json!({ "deleted": deleted }))
        }

        "choreo_run_ceremony" => {
            let request = build_run_ceremony_request(arguments)?;
            let response = client
                .run_ceremony(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::run_ceremony_response_to_json(response.into_inner()))
        }

        // The read side of a working session. The response is the
        // same shape the in-process backend renders, which is the
        // whole point: one tool, either backend, one answer.
        "choreo_get_ceremony_instance" => {
            let obj = j2p::require_object(arguments, "tools/call.arguments")?;
            let request = pb::GetCeremonyInstanceRequest {
                ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
            };
            let response = client
                .get_ceremony_instance(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::GetCeremonyInstanceResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_list_ceremony_instances" => {
            let response = client
                .list_ceremony_instances(pb::ListCeremonyInstancesRequest {})
                .await
                .map_err(|s| status_error(&s))?;
            let pb::ListCeremonyInstancesResponse { instances } = response.into_inner();
            Ok(json!({
                "instances": instances
                    .into_iter()
                    .map(p2j::ceremony_instance_state_to_json)
                    .collect::<Vec<_>>(),
            }))
        }

        // Every move answers with the session, so one converter serves
        // them all — the same shape the in-process backend renders.
        "choreo_start_ceremony" => {
            let request = build_start_ceremony_request(arguments)?;
            let response = client
                .start_ceremony(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::StartCeremonyResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_start_published_ceremony" => {
            let request = build_start_published_ceremony_request(arguments)?;
            let response = client
                .start_published_ceremony(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::StartPublishedCeremonyResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_run_ceremony_step" => {
            let request = build_run_ceremony_step_request(arguments)?;
            let response = client
                .run_ceremony_step(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::RunCeremonyStepResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_apply_ceremony_transition" => {
            let request = build_apply_ceremony_transition_request(arguments)?;
            let response = client
                .apply_ceremony_transition(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::ApplyCeremonyTransitionResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_approve_ceremony_guard" => {
            let request = build_approve_ceremony_guard_request(arguments)?;
            let response = client
                .approve_ceremony_guard(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::ApproveCeremonyGuardResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_defer_ceremony_guard" => {
            let request = build_defer_ceremony_guard_request(arguments)?;
            let response = client
                .defer_ceremony_guard(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::DeferCeremonyGuardResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_assert_ceremony_reason" => {
            let request = build_assert_ceremony_reason_request(arguments)?;
            let response = client
                .assert_ceremony_reason(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::AssertCeremonyReasonResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_request_ceremony_intervention" => {
            let request = build_request_ceremony_intervention_request(arguments)?;
            let response = client
                .request_ceremony_intervention(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::RequestCeremonyInterventionResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_respond_to_ceremony_intervention" => {
            let request = build_respond_to_ceremony_intervention_request(arguments)?;
            let response = client
                .respond_to_ceremony_intervention(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::RespondToCeremonyInterventionResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_close_ceremony_intervention" => {
            let request = build_close_ceremony_intervention_request(arguments)?;
            let response = client
                .close_ceremony_intervention(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::CloseCeremonyInterventionResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_collect_ceremony_evidence" => {
            let request = build_collect_ceremony_evidence_request(arguments)?;
            let response = client
                .collect_ceremony_evidence(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::CollectCeremonyEvidenceResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        // Authoring. Validate and explain answer about the YAML in the
        // request; publishing is what puts a version in the catalogue.
        "choreo_validate_ceremony_draft" => {
            let request = pb::ValidateCeremonyDraftRequest {
                definition_yaml: definition_yaml(arguments)?,
            };
            let response = client
                .validate_ceremony_draft(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::validate_ceremony_draft_to_json(response.into_inner()))
        }

        "choreo_explain_ceremony_draft" => {
            let request = pb::ExplainCeremonyDraftRequest {
                definition_yaml: definition_yaml(arguments)?,
            };
            let response = client
                .explain_ceremony_draft(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::explain_ceremony_draft_to_json(&response.into_inner()))
        }

        "choreo_publish_ceremony_definition" => {
            let request = pb::PublishCeremonyDefinitionRequest {
                definition_yaml: definition_yaml(arguments)?,
            };
            let response = client
                .publish_ceremony_definition(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::publish_ceremony_definition_to_json(
                &response.into_inner(),
            ))
        }

        "choreo_diff_ceremony_definitions" => {
            let obj = j2p::require_object(arguments, "tools/call.arguments")?;
            let request = pb::DiffCeremonyDefinitionsRequest {
                before: definition_ref(obj, "before")?,
                after: definition_ref(obj, "after")?,
            };
            let response = client
                .diff_ceremony_definitions(request)
                .await
                .map_err(|s| status_error(&s))?;
            Ok(p2j::diff_ceremony_definitions_to_json(
                response.into_inner(),
            ))
        }

        "choreo_bind_ceremony_participants" => {
            let obj = j2p::require_object(arguments, "tools/call.arguments")?;
            let seating = obj
                .get("seating")
                .and_then(Value::as_object)
                .ok_or_else(|| "missing required object `seating`".to_owned())?;
            let request = pb::BindCeremonyParticipantsRequest {
                actor_id: j2p::require_str(obj, "actor_id")?.to_owned(),
                actor_kind: j2p::require_str(obj, "actor_kind")?.to_owned(),
                ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
                seating: seating
                    .iter()
                    .map(|(role, specialty)| {
                        specialty
                            .as_str()
                            .map(|specialty| (role.clone(), specialty.to_owned()))
                            .ok_or_else(|| format!("`seating.{role}` must be a string"))
                    })
                    .collect::<Result<_, _>>()?,
            };
            let response = client
                .bind_ceremony_participants(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::BindCeremonyParticipantsResponse { instance } = response.into_inner();
            instance
                .map(p2j::ceremony_instance_state_to_json)
                .ok_or_else(|| "choreographer returned no ceremony instance".to_owned())
        }

        "choreo_get_status" => {
            let request = build_get_status_request(arguments);
            let response = client
                .get_status(request)
                .await
                .map_err(|s| status_error(&s))?;
            let pb::GetStatusResponse {
                version,
                uptime_seconds,
                health,
                stats,
            } = response.into_inner();
            Ok(json!({
                "version": version,
                "uptime_seconds": uptime_seconds,
                "health": health,
                "stats": stats.map_or(Value::Null, p2j::statistics_to_json),
            }))
        }

        "choreo_get_metrics" => {
            let response = client
                .get_metrics(pb::GetMetricsRequest {})
                .await
                .map_err(|s| status_error(&s))?;
            let pb::GetMetricsResponse { stats } = response.into_inner();
            Ok(json!({
                "stats": stats.map_or(Value::Null, p2j::statistics_to_json),
            }))
        }

        other => Err(format!("unknown choreo MCP tool `{other}`")),
    }
}

fn status_error(status: &tonic::Status) -> String {
    format!("gRPC {}: {}", status.code(), status.message())
}

// ---------------------------------------------------------------------------
// Request builders. Each takes the raw `tools/call.arguments` JSON
// value and produces a typed proto request. Validation errors come
// back as plain strings; tonic gets a fully-formed proto.
// ---------------------------------------------------------------------------

fn build_deliberate_request(args: &Value) -> Result<pb::DeliberateRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    let task_value = obj
        .get("task")
        .ok_or_else(|| "missing required `task` object".to_string())?;
    Ok(pb::DeliberateRequest {
        task: Some(j2p::task_from_json(task_value)?),
    })
}

fn build_stream_deliberation_request(
    args: &Value,
) -> Result<pb::StreamDeliberationRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    let task_value = obj
        .get("task")
        .ok_or_else(|| "missing required `task` object".to_string())?;
    Ok(pb::StreamDeliberationRequest {
        task: Some(j2p::task_from_json(task_value)?),
    })
}

fn build_get_deliberation_result_request(
    args: &Value,
) -> Result<pb::GetDeliberationResultRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::GetDeliberationResultRequest {
        task_id: j2p::require_str(obj, "task_id")?.to_string(),
    })
}

fn build_orchestrate_request(args: &Value) -> Result<pb::OrchestrateRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    let task_value = obj
        .get("task")
        .ok_or_else(|| "missing required `task` object".to_string())?;
    Ok(pb::OrchestrateRequest {
        task: Some(j2p::task_from_json(task_value)?),
        execution_options: j2p::optional_pb_struct(obj, "execution_options")?,
    })
}

fn build_create_council_request(args: &Value) -> Result<pb::CreateCouncilRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::CreateCouncilRequest {
        specialty: j2p::require_str(obj, "specialty")?.to_string(),
        num_agents: j2p::optional_u32(obj, "num_agents")?,
        agent_config: j2p::optional_pb_struct(obj, "agent_config")?,
    })
}

fn build_list_councils_request(args: &Value) -> pb::ListCouncilsRequest {
    let include_agents = args
        .as_object()
        .is_some_and(|obj| j2p::optional_bool(obj, "include_agents"));
    pb::ListCouncilsRequest { include_agents }
}

fn build_delete_council_request(args: &Value) -> Result<pb::DeleteCouncilRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::DeleteCouncilRequest {
        specialty: j2p::require_str(obj, "specialty")?.to_string(),
    })
}

fn build_register_agent_request(args: &Value) -> Result<pb::RegisterAgentRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    let agent_value = obj
        .get("agent")
        .ok_or_else(|| "missing required `agent` object".to_string())?;
    Ok(pb::RegisterAgentRequest {
        specialty: j2p::require_str(obj, "specialty")?.to_string(),
        agent: Some(j2p::agent_summary_from_json(agent_value)?),
        agent_config: j2p::optional_pb_struct(obj, "agent_config")?,
    })
}

fn build_unregister_agent_request(args: &Value) -> Result<pb::UnregisterAgentRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::UnregisterAgentRequest {
        agent_id: j2p::require_str(obj, "agent_id")?.to_string(),
    })
}

fn build_process_trigger_event_request(
    args: &Value,
) -> Result<pb::ProcessTriggerEventRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    let event_value = obj
        .get("event")
        .ok_or_else(|| "missing required `event` object".to_string())?;
    Ok(pb::ProcessTriggerEventRequest {
        event: Some(j2p::trigger_event_from_json(event_value)?),
    })
}

fn build_get_status_request(args: &Value) -> pb::GetStatusRequest {
    let include_stats = args
        .as_object()
        .is_some_and(|obj| j2p::optional_bool(obj, "include_stats"));
    pb::GetStatusRequest { include_stats }
}

fn build_run_council_decision_request(
    args: &Value,
) -> Result<pb::RunCouncilDecisionRequest, String> {
    j2p::run_council_decision_request_from_json(args)
}

fn build_run_ceremony_request(args: &Value) -> Result<pb::RunCeremonyRequest, String> {
    j2p::run_ceremony_request_from_json(args)
}

fn build_register_contract_request(args: &Value) -> Result<pb::RegisterContractRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    let contract_value = obj
        .get("contract")
        .ok_or_else(|| "missing required `contract` object".to_string())?;
    Ok(pb::RegisterContractRequest {
        contract: Some(j2p::output_contract_from_json(contract_value)?),
    })
}

fn build_delete_contract_request(args: &Value) -> Result<pb::DeleteContractRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::DeleteContractRequest {
        contract_id: j2p::require_str(obj, "contract_id")?.to_string(),
    })
}

fn definition_yaml(args: &Value) -> Result<String, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(j2p::require_str(obj, "definition_yaml")?.to_owned())
}

/// Minting an id client-side when the caller left it out, exactly as
/// the in-process backend does: a tool that demands an identifier for
/// a thing that does not exist yet is a tool that makes its caller
/// invent one.
fn minted_id(obj: &serde_json::Map<String, Value>, key: &str) -> String {
    j2p::optional_str(obj, key)
        .filter(|value| !value.trim().is_empty())
        .map_or_else(|| Uuid::new_v4().to_string(), ToOwned::to_owned)
}

fn build_start_ceremony_request(args: &Value) -> Result<pb::StartCeremonyRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::StartCeremonyRequest {
        actor_id: j2p::require_str(obj, "actor_id")?.to_owned(),
        actor_kind: j2p::require_str(obj, "actor_kind")?.to_owned(),
        ceremony_id: minted_id(obj, "ceremony_id"),
        definition_yaml: j2p::require_str(obj, "definition_yaml")?.to_owned(),
        context: j2p::optional_pb_struct(obj, "context")?,
    })
}

fn build_start_published_ceremony_request(
    args: &Value,
) -> Result<pb::StartPublishedCeremonyRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::StartPublishedCeremonyRequest {
        actor_id: j2p::require_str(obj, "actor_id")?.to_owned(),
        actor_kind: j2p::require_str(obj, "actor_kind")?.to_owned(),
        ceremony_id: minted_id(obj, "ceremony_id"),
        ceremony: j2p::require_str(obj, "ceremony")?.to_owned(),
        version: j2p::require_str(obj, "version")?.to_owned(),
        context: j2p::optional_pb_struct(obj, "context")?,
    })
}

fn build_run_ceremony_step_request(args: &Value) -> Result<pb::RunCeremonyStepRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::RunCeremonyStepRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        actor_kind: j2p::require_str(obj, "actor_kind")?.to_owned(),
        step_id: j2p::require_str(obj, "step_id")?.to_owned(),
        lease_owner_id: j2p::optional_str(obj, "lease_owner_id")
            .unwrap_or_default()
            .to_owned(),
        idempotency_key: j2p::optional_str(obj, "idempotency_key")
            .unwrap_or_default()
            .to_owned(),
        lease_ttl_ms: j2p::optional_u64(obj, "lease_ttl_ms")?,
    })
}

fn build_apply_ceremony_transition_request(
    args: &Value,
) -> Result<pb::ApplyCeremonyTransitionRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::ApplyCeremonyTransitionRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        trigger: j2p::require_str(obj, "trigger")?.to_owned(),
        actor_kind: j2p::require_str(obj, "actor_kind")?.to_owned(),
    })
}

fn build_approve_ceremony_guard_request(
    args: &Value,
) -> Result<pb::ApproveCeremonyGuardRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::ApproveCeremonyGuardRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        guard_name: j2p::require_str(obj, "guard_name")?.to_owned(),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
    })
}

/// One end of a reason, from the tool's JSON.
///
/// Only the field the kind names is read, which is what the wire does
/// too — the discriminator is what the object means.
fn build_ceremony_record_ref(
    value: &Value,
    field: &str,
) -> Result<pb::CeremonyRecordRefState, String> {
    let obj = j2p::require_object(value, field)?;
    Ok(pb::CeremonyRecordRefState {
        kind: j2p::require_str(obj, "kind")?.to_owned(),
        step_id: obj
            .get("step_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        agenda_item: obj
            .get("agenda_item")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        ordinal: obj
            .get("ordinal")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default(),
        guard_name: obj
            .get("guard_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn build_assert_ceremony_reason_request(
    args: &Value,
) -> Result<pb::AssertCeremonyReasonRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::AssertCeremonyReasonRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
        from: Some(build_ceremony_record_ref(
            obj.get("from").ok_or("`from` is required")?,
            "from",
        )?),
        to: Some(build_ceremony_record_ref(
            obj.get("to").ok_or("`to` is required")?,
            "to",
        )?),
        kind: j2p::require_str(obj, "kind")?.to_owned(),
        why: j2p::require_str(obj, "why")?.to_owned(),
        confidence: j2p::require_str(obj, "confidence")?.to_owned(),
    })
}

fn build_defer_ceremony_guard_request(
    args: &Value,
) -> Result<pb::DeferCeremonyGuardRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::DeferCeremonyGuardRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        guard_name: j2p::require_str(obj, "guard_name")?.to_owned(),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
        statement: j2p::require_str(obj, "statement")?.to_owned(),
        reason: j2p::require_str(obj, "reason")?.to_owned(),
        reconsider_when: j2p::string_array(obj, "reconsider_when"),
    })
}

fn build_request_ceremony_intervention_request(
    args: &Value,
) -> Result<pb::RequestCeremonyInterventionRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::RequestCeremonyInterventionRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        intervention_id: minted_id(obj, "intervention_id"),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
        kind: j2p::require_str(obj, "kind")?.to_owned(),
        target_role_ids: j2p::string_array(obj, "target_role_ids"),
        message: j2p::require_str(obj, "message")?.to_owned(),
        details: j2p::optional_pb_struct(obj, "details")?,
        provenance: provenance_from_json(obj)?,
    })
}

fn provenance_from_json(
    obj: &serde_json::Map<String, Value>,
) -> Result<Option<pb::CeremonyInterventionProvenanceState>, String> {
    let Some(value) = obj.get("provenance") else {
        return Ok(None);
    };
    let provenance = j2p::require_object(value, "provenance")?;
    Ok(Some(pb::CeremonyInterventionProvenanceState {
        source_intervention_id: j2p::require_str(provenance, "source_intervention_id")?.to_owned(),
        source_response_role_id: j2p::require_str(provenance, "source_response_role_id")?
            .to_owned(),
        selected_role_id: j2p::require_str(provenance, "selected_role_id")?.to_owned(),
    }))
}

fn build_respond_to_ceremony_intervention_request(
    args: &Value,
) -> Result<pb::RespondToCeremonyInterventionRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::RespondToCeremonyInterventionRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
        intervention_id: j2p::require_str(obj, "intervention_id")?.to_owned(),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
        message: j2p::require_str(obj, "message")?.to_owned(),
        details: j2p::optional_pb_struct(obj, "details")?,
    })
}

fn build_close_ceremony_intervention_request(
    args: &Value,
) -> Result<pb::CloseCeremonyInterventionRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::CloseCeremonyInterventionRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
        intervention_id: j2p::require_str(obj, "intervention_id")?.to_owned(),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
    })
}

fn build_collect_ceremony_evidence_request(
    args: &Value,
) -> Result<pb::CollectCeremonyEvidenceRequest, String> {
    let obj = j2p::require_object(args, "tools/call.arguments")?;
    Ok(pb::CollectCeremonyEvidenceRequest {
        ceremony_id: j2p::require_str(obj, "ceremony_id")?.to_owned(),
        role_kind: j2p::require_str(obj, "role_kind")?.to_owned(),
        intervention_id: j2p::require_str(obj, "intervention_id")?.to_owned(),
        role_id: j2p::require_str(obj, "role_id")?.to_owned(),
        source_id: j2p::require_str(obj, "source_id")?.to_owned(),
        query: j2p::require_str(obj, "query")?.to_owned(),
        details: j2p::optional_pb_struct(obj, "details")?,
    })
}

/// One side of a comparison. Absent is an error here rather than a
/// default: there is no sensible definition to compare against when
/// the caller named none.
fn definition_ref(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<pb::CeremonyDefinitionRef>, String> {
    let value = obj
        .get(key)
        .ok_or_else(|| format!("missing required object `{key}`"))?;
    let reference = j2p::require_object(value, key)?;
    Ok(Some(pb::CeremonyDefinitionRef {
        ceremony: j2p::optional_str(reference, "ceremony")
            .unwrap_or_default()
            .to_owned(),
        version: j2p::optional_str(reference, "version")
            .unwrap_or_default()
            .to_owned(),
        definition_yaml: j2p::optional_str(reference, "definition_yaml")
            .unwrap_or_default()
            .to_owned(),
    }))
}

#[cfg(test)]
mod schema_gate {
    //! Every field a request builder demands must be declared required
    //! in the schema that tool publishes.
    //!
    //! Twice now a tool has required a field its published schema said
    //! nothing about: `role_kind` on the intervention verbs, and
    //! `actor_id` on `choreo_start_ceremony`, whose declaration landed
    //! on an identically-shaped block one function away. Both shipped
    //! green, because nothing compared the two sides.
    //!
    //! The check is behavioural rather than structural: build the
    //! arguments a caller reading the schema would send — exactly the
    //! declared required fields, nothing else — and hand them to the
    //! builder. A builder that answers "missing required string `x`"
    //! is asking for something the schema never told anyone to send.

    use serde_json::{json, Map, Value};

    use crate::protocol::tools_list_result;

    /// Tools whose builder takes no arguments worth declaring.
    ///
    /// Listed rather than skipped by accident: a new tool has to be
    /// put on one side of this line or the other.
    const NO_ARGUMENTS: [&str; 4] = [
        "choreo_list_councils",
        "choreo_list_contracts",
        "choreo_get_status",
        "choreo_get_metrics",
    ];

    /// What a caller reading this schema would send, and no more.
    ///
    /// A `oneOf` or `anyOf` of alternative requirements is satisfied by
    /// its first branch. Picking one is the point: a schema that states
    /// its alternatives can be driven, and one that leaves them to
    /// prose cannot.
    fn minimal_arguments(schema: &Value) -> Value {
        let mut required = schema
            .get("required")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for key in ["oneOf", "anyOf"] {
            if let Some(branch) = schema
                .get(key)
                .and_then(Value::as_array)
                .and_then(|branches| branches.first())
                .and_then(|branch| branch.get("required"))
                .and_then(Value::as_array)
            {
                required.extend(branch.iter().cloned());
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        let mut arguments = Map::new();
        for field in required {
            let Some(name) = field.as_str() else { continue };
            let property = properties.and_then(|properties| properties.get(name));
            arguments.insert(name.to_owned(), value_for(property));
        }
        Value::Object(arguments)
    }

    /// A plausible value of the declared type.
    ///
    /// Plausible, not valid: the builders check presence and shape, and
    /// domain validation happens further in. A value that gets past the
    /// builder is all this needs.
    fn value_for(property: Option<&Value>) -> Value {
        let Some(property) = property else {
            return json!("x");
        };
        if let Some(first) = property
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|variants| variants.first())
        {
            return first.clone();
        }
        match property.get("type").and_then(Value::as_str) {
            Some("object") => minimal_arguments(property),
            Some("array") => json!([]),
            Some("integer" | "number") => json!(1),
            Some("boolean") => json!(true),
            _ => json!("x"),
        }
    }

    #[test]
    fn no_builder_demands_a_field_its_schema_never_declared() {
        let catalog = tools_list_result(|_| true);
        let tools = catalog["tools"].as_array().expect("a tool catalog");
        assert!(!tools.is_empty(), "the catalog served no tools");

        let mut unchecked = Vec::new();
        for tool in tools {
            let name = tool["name"].as_str().expect("every tool is named");
            if NO_ARGUMENTS.contains(&name) {
                continue;
            }
            let arguments = minimal_arguments(&tool["inputSchema"]);
            let Some(outcome) = built(name, &arguments) else {
                unchecked.push(name.to_owned());
                continue;
            };
            if let Err(complaint) = outcome {
                assert!(
                    !complaint.contains("missing required"),
                    "{name} demands something its schema does not declare: {complaint}\n\
                     sent exactly what the schema asks for: {arguments}"
                );
            }
        }

        // Not a failure, but not silence either: a tool whose builder
        // this gate cannot drive is a tool the gate is not covering,
        // and saying so beats reading a green test as full coverage.
        if !unchecked.is_empty() {
            eprintln!("schema gate could not drive: {unchecked:?}");
        }
    }

    /// The request a builder makes of these arguments, rendered, or
    /// what it complained about instead.
    ///
    /// Rendered rather than typed because every builder returns a
    /// different message and this gate holds them all the same way.
    fn built(name: &str, arguments: &Value) -> Option<Result<String, String>> {
        let outcome = match name {
            "choreo_deliberate" => rendered(build_deliberate_request(arguments)),
            "choreo_stream_deliberation" => rendered(build_stream_deliberation_request(arguments)),
            "choreo_get_deliberation_result" => {
                rendered(build_get_deliberation_result_request(arguments))
            }
            "choreo_orchestrate" => rendered(build_orchestrate_request(arguments)),
            "choreo_create_council" => rendered(build_create_council_request(arguments)),
            "choreo_delete_council" => rendered(build_delete_council_request(arguments)),
            "choreo_register_agent" => rendered(build_register_agent_request(arguments)),
            "choreo_unregister_agent" => rendered(build_unregister_agent_request(arguments)),
            "choreo_process_trigger_event" => {
                rendered(build_process_trigger_event_request(arguments))
            }
            "choreo_run_council_decision" => {
                rendered(build_run_council_decision_request(arguments))
            }
            "choreo_register_contract" => rendered(build_register_contract_request(arguments)),
            "choreo_delete_contract" => rendered(build_delete_contract_request(arguments)),
            "choreo_run_ceremony" => rendered(build_run_ceremony_request(arguments)),
            "choreo_start_ceremony" => rendered(build_start_ceremony_request(arguments)),
            "choreo_start_published_ceremony" => {
                rendered(build_start_published_ceremony_request(arguments))
            }
            "choreo_run_ceremony_step" => rendered(build_run_ceremony_step_request(arguments)),
            "choreo_apply_ceremony_transition" => {
                rendered(build_apply_ceremony_transition_request(arguments))
            }
            "choreo_approve_ceremony_guard" => {
                rendered(build_approve_ceremony_guard_request(arguments))
            }
            "choreo_defer_ceremony_guard" => {
                rendered(build_defer_ceremony_guard_request(arguments))
            }
            "choreo_request_ceremony_intervention" => {
                rendered(build_request_ceremony_intervention_request(arguments))
            }
            "choreo_respond_to_ceremony_intervention" => {
                rendered(build_respond_to_ceremony_intervention_request(arguments))
            }
            "choreo_close_ceremony_intervention" => {
                rendered(build_close_ceremony_intervention_request(arguments))
            }
            "choreo_collect_ceremony_evidence" => {
                rendered(build_collect_ceremony_evidence_request(arguments))
            }
            "choreo_assert_ceremony_reason" => {
                rendered(build_assert_ceremony_reason_request(arguments))
            }
            _ => return None,
        };
        Some(outcome)
    }

    fn rendered<T: std::fmt::Debug>(outcome: Result<T, String>) -> Result<String, String> {
        outcome.map(|request| format!("{request:?}"))
    }

    /// A field the schema calls required must reach the request.
    ///
    /// The other way this pair can disagree, and the one that is worse:
    /// a builder that fills a declared field with a value of its own
    /// never complains about anything. `choreo_run_ceremony` shipped
    /// like that — every call sent `actor_id: "operator-1"` whatever the
    /// caller declared, which is the engine writing down an actor
    /// nobody chose, the one thing the field exists to prevent.
    ///
    /// Told apart by building twice with different values: a builder
    /// that reads the field produces two different requests, and one
    /// that invents produces the same one twice.
    #[test]
    fn every_required_field_reaches_the_request() {
        let catalog = tools_list_result(|_| true);
        for tool in catalog["tools"].as_array().expect("a tool catalog") {
            let name = tool["name"].as_str().expect("every tool is named");
            if NO_ARGUMENTS.contains(&name) {
                continue;
            }
            let schema = &tool["inputSchema"];
            let baseline = minimal_arguments(schema);
            let Some(fields) = baseline.as_object().map(|fields| {
                fields
                    .iter()
                    .filter(|(_, value)| value.is_string())
                    .map(|(field, _)| field.clone())
                    .collect::<Vec<_>>()
            }) else {
                continue;
            };

            for field in fields {
                // Enums have a fixed vocabulary, so "another value" is
                // not something this gate can invent for them.
                if schema["properties"][&field].get("enum").is_some() {
                    continue;
                }
                let mut other = baseline.clone();
                other[&field] = json!("y");
                let (Some(Ok(first)), Some(Ok(second))) =
                    (built(name, &baseline), built(name, &other))
                else {
                    continue;
                };
                assert_ne!(
                    first, second,
                    "{name} builds the same request whether `{field}` says one thing or another: \
                     it is not reading the field it declares required"
                );
            }
        }
    }

    use super::{
        build_apply_ceremony_transition_request, build_approve_ceremony_guard_request,
        build_assert_ceremony_reason_request, build_close_ceremony_intervention_request,
        build_collect_ceremony_evidence_request, build_create_council_request,
        build_defer_ceremony_guard_request, build_delete_contract_request,
        build_delete_council_request, build_deliberate_request,
        build_get_deliberation_result_request, build_orchestrate_request,
        build_process_trigger_event_request, build_register_agent_request,
        build_register_contract_request, build_request_ceremony_intervention_request,
        build_respond_to_ceremony_intervention_request, build_run_ceremony_request,
        build_run_ceremony_step_request, build_run_council_decision_request,
        build_start_ceremony_request, build_start_published_ceremony_request,
        build_stream_deliberation_request, build_unregister_agent_request,
    };
}
