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

use super::json_to_proto as j2p;
use super::proto_to_json as p2j;
use super::streaming;

/// Dispatch one tool call. Returns the **structured content** of the
/// MCP tool result (just the JSON; the caller wraps it in
/// `tool_success_result`).
#[allow(clippy::too_many_lines)] // 16 tool arms; splitting fragments the dispatch table
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
