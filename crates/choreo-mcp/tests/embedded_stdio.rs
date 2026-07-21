#![cfg(feature = "embedded")]

use std::process::Stdio;
use std::time::Duration;

use choreo_mcp::ChoreoMcpServer;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

const CEREMONY_YAML: &str = r#"
version: "1.0"
name: "codex_plugin_smoke"
states:
  - id: STARTED
    initial: true
  - id: COMPLETED
    terminal: true
transitions:
  - from: STARTED
    to: COMPLETED
    trigger: finish
    guards:
      - work_completed
steps:
  - id: work
    state: STARTED
    handler: embedded_noop
guards:
  work_completed:
    type: automated
    check: "step_status:work:COMPLETED"
roles:
  - id: SYSTEM
    allowed_actions:
      - work
      - finish
"#;

const HUMAN_APPROVAL_CEREMONY_YAML: &str = r#"
version: "1.0"
name: "human_approval_smoke"
states:
  - id: INVESTIGATING
    initial: true
  - id: COMPLETED
    terminal: true
transitions:
  - from: INVESTIGATING
    to: COMPLETED
    trigger: finish
    guards:
      - investigation_completed
      - engineer_authorized
steps:
  - id: investigate
    state: INVESTIGATING
    handler: embedded_noop
guards:
  investigation_completed:
    type: automated
    check: "step_status:investigate:COMPLETED"
  engineer_authorized:
    type: human
    check: manual_approval
roles:
  - id: ENGINEER
    allowed_actions:
      - investigate
      - finish
"#;

const COLLABORATIVE_TABLE_CEREMONY_YAML: &str = r#"
version: "1.0"
name: "collaborative_incident_table"
states:
  - id: INVESTIGATING
    initial: true
  - id: COMPLETED
    terminal: true
transitions:
  - from: INVESTIGATING
    to: COMPLETED
    trigger: finish
steps:
  - id: investigate
    state: INVESTIGATING
    handler: embedded_noop
roles:
  - id: ENGINEER
    allowed_actions:
      - investigate
      - finish
      - request_intervention
  - id: OBSERVER
    allowed_actions:
      - respond_to_intervention
  - id: DATABASE_SPECIALIST
    allowed_actions:
      - respond_to_intervention
  - id: QUEUE_SPECIALIST
    allowed_actions:
      - respond_to_intervention
"#;

#[tokio::test]
async fn embedded_server_advertises_only_executable_tools() {
    let server = ChoreoMcpServer::embedded();

    let initialized = send(&server, jsonrpc(1, "initialize", None)).await;
    assert_eq!(initialized["result"]["metadata"]["backend"], "embedded");

    let tools = send(&server, jsonrpc(2, "tools/list", None)).await;
    let catalog = tools["result"]["tools"].as_array().unwrap();
    let names = catalog
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "choreo_run_ceremony",
            "choreo_start_ceremony",
            "choreo_run_ceremony_step",
            "choreo_approve_ceremony_guard",
            "choreo_defer_ceremony_guard",
            "choreo_apply_ceremony_transition",
            "choreo_get_ceremony_instance",
            "choreo_request_ceremony_intervention",
            "choreo_respond_to_ceremony_intervention",
            "choreo_close_ceremony_intervention",
        ]
    );

    let completed = send(&server, run_ceremony_call(3, "embedded-direct-smoke")).await;
    assert_completed(&completed);
}

#[tokio::test]
async fn engineer_dynamically_opens_and_controls_collaborative_table_interventions() {
    let server = ChoreoMcpServer::embedded();
    let ceremony_id = "embedded-collaborative-table";

    let started = send(&server, start_collaborative_ceremony_call(1, ceremony_id)).await;
    assert_eq!(structured(&started)["interventions"], json!([]));

    complete_table_opinion(&server, ceremony_id).await;
    inspect_queue_as_targeted_role(&server, ceremony_id).await;
}

async fn complete_table_opinion(server: &ChoreoMcpServer, ceremony_id: &str) {
    let requested = send(
        server,
        request_intervention_call(
            2,
            ceremony_id,
            "table-opinion",
            "opinion",
            None,
            "What failure mode best explains the current symptoms?",
            &json!({"incident_ref": "INC-42"}),
            None,
        ),
    )
    .await;
    assert_eq!(
        structured(&requested)["open_intervention_ids"],
        json!(["table-opinion"])
    );
    assert_eq!(
        structured(&requested)["interventions"][0]["target"]["kind"],
        "table"
    );

    let observed = send(
        server,
        respond_intervention_call(
            3,
            ceremony_id,
            "table-opinion",
            "OBSERVER",
            "The timing suggests downstream saturation; ask the queue specialist to inspect depth.",
            &json!({"confidence": 65}),
        ),
    )
    .await;
    assert_eq!(
        structured(&observed)["interventions"][0]["responses"][0]["role_id"],
        "OBSERVER"
    );
    let database = send(
        server,
        respond_intervention_call(
            4,
            ceremony_id,
            "table-opinion",
            "DATABASE_SPECIALIST",
            "Connection saturation is plausible but not yet evidenced.",
            &json!({}),
        ),
    )
    .await;
    assert_eq!(
        structured(&database)["interventions"][0]["responses"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let duplicate = send(
        server,
        respond_intervention_call(
            5,
            ceremony_id,
            "table-opinion",
            "OBSERVER",
            "A second answer must not replace the first.",
            &json!({}),
        ),
    )
    .await;
    assert_eq!(duplicate["result"]["isError"], true);

    let unauthorized_close = send(
        server,
        close_intervention_call(6, ceremony_id, "table-opinion", "DATABASE_SPECIALIST"),
    )
    .await;
    assert_eq!(unauthorized_close["result"]["isError"], true);

    let closed = send(
        server,
        close_intervention_call(7, ceremony_id, "table-opinion", "ENGINEER"),
    )
    .await;
    assert_eq!(structured(&closed)["interventions"][0]["status"], "closed");
}

async fn inspect_queue_as_targeted_role(server: &ChoreoMcpServer, ceremony_id: &str) {
    let ungrounded_request = send(
        server,
        request_intervention_call(
            8,
            ceremony_id,
            "inspect-queue-ungrounded",
            "investigation",
            Some(&["QUEUE_SPECIALIST"]),
            "Inspect queue depth without consuming messages.",
            &json!({"queue": "orders"}),
            Some(&json!({
                "source_intervention_id": "table-opinion",
                "source_response_role_id": "QUEUE_SPECIALIST",
                "selected_role_id": "QUEUE_SPECIALIST",
            })),
        ),
    )
    .await;
    assert_eq!(ungrounded_request["result"]["isError"], true);

    let queue_request = send(
        server,
        request_intervention_call(
            9,
            ceremony_id,
            "inspect-queue",
            "investigation",
            Some(&["QUEUE_SPECIALIST"]),
            "Inspect queue depth without consuming messages.",
            &json!({"queue": "orders"}),
            Some(&json!({
                "source_intervention_id": "table-opinion",
                "source_response_role_id": "OBSERVER",
                "selected_role_id": "QUEUE_SPECIALIST",
            })),
        ),
    )
    .await;
    assert_eq!(
        structured(&queue_request)["interventions"][1]["target"]["role_ids"],
        json!(["QUEUE_SPECIALIST"])
    );
    assert_eq!(
        structured(&queue_request)["interventions"][1]["provenance"],
        json!({
            "source_intervention_id": "table-opinion",
            "source_response_role_id": "OBSERVER",
            "selected_role_id": "QUEUE_SPECIALIST",
        })
    );

    let unrelated = send(
        server,
        respond_intervention_call(
            10,
            ceremony_id,
            "inspect-queue",
            "OBSERVER",
            "I was not targeted.",
            &json!({}),
        ),
    )
    .await;
    assert_eq!(unrelated["result"]["isError"], true);

    let queue_response = send(
        server,
        respond_intervention_call(
            11,
            ceremony_id,
            "inspect-queue",
            "QUEUE_SPECIALIST",
            "Depth is 12,400 and oldest age is 94 seconds; no messages consumed.",
            &json!({"depth": 12400, "oldest_age_seconds": 94, "read_only": true}),
        ),
    )
    .await;
    let interventions = structured(&queue_response)["interventions"]
        .as_array()
        .unwrap();
    assert_eq!(interventions.len(), 2);
    assert_eq!(interventions[0]["intervention_id"], "table-opinion");
    assert_eq!(interventions[1]["intervention_id"], "inspect-queue");
    assert_eq!(
        interventions[1]["responses"][0]["details"]["read_only"],
        true
    );
}

#[tokio::test]
async fn embedded_server_pauses_until_a_human_guard_is_approved() {
    let server = ChoreoMcpServer::embedded();
    let ceremony_id = "embedded-human-approval";

    let started = send(&server, start_ceremony_call(1, ceremony_id)).await;
    assert_eq!(structured(&started)["current_state"], "INVESTIGATING");
    assert_eq!(structured(&started)["next_step_id"], "investigate");
    assert_eq!(structured(&started)["waiting_for_human"], json!([]));

    let stepped = send(&server, run_step_call(2, ceremony_id, "investigate")).await;
    assert!(structured(&stepped)["next_step_id"].is_null());
    assert_eq!(
        structured(&stepped)["waiting_for_human"],
        json!(["engineer_authorized"])
    );

    let deferred = send(
        &server,
        deferral_call(3, ceremony_id, "engineer_authorized"),
    )
    .await;
    assert_eq!(
        structured(&deferred)["waiting_for_human"],
        json!(["engineer_authorized"])
    );
    assert_eq!(structured(&deferred)["transitions"][0]["enabled"], false);
    assert!(structured(&deferred)["context"]["engineer_authorized"].is_null());
    assert_eq!(
        structured(&deferred)["guard_deferrals"][0]["statement"],
        "I do not know."
    );
    assert_eq!(
        structured(&deferred)["guard_deferrals"][0]["reason"],
        "The resolution is not clear."
    );

    let blocked = send(&server, transition_call(4, ceremony_id, "finish")).await;
    assert_eq!(blocked["result"]["isError"], true);

    let inspected = send(&server, instance_call(5, ceremony_id)).await;
    assert_eq!(
        structured(&inspected)["waiting_for_human"],
        json!(["engineer_authorized"])
    );

    let rejected = send(
        &server,
        approval_call(6, ceremony_id, "investigation_completed"),
    )
    .await;
    assert_eq!(rejected["result"]["isError"], true);

    let approved = send(
        &server,
        approval_call(7, ceremony_id, "engineer_authorized"),
    )
    .await;
    assert_eq!(structured(&approved)["waiting_for_human"], json!([]));
    assert_eq!(structured(&approved)["transitions"][0]["enabled"], true);

    let completed = send(&server, transition_call(8, ceremony_id, "finish")).await;
    assert_eq!(structured(&completed)["completed"], true);
    assert_eq!(structured(&completed)["current_state"], "COMPLETED");
}

#[tokio::test]
async fn embedded_binary_completes_incremental_human_authorization_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_choreo-mcp"))
        .env("CHOREO_MCP_BACKEND", "embedded")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    write_request(&mut stdin, jsonrpc(1, "initialize", None)).await;
    write_request(&mut stdin, jsonrpc(2, "tools/list", None)).await;
    write_request(&mut stdin, start_ceremony_call(3, "embedded-stdio-human")).await;
    write_request(
        &mut stdin,
        run_step_call(4, "embedded-stdio-human", "investigate"),
    )
    .await;
    write_request(
        &mut stdin,
        approval_call(5, "embedded-stdio-human", "engineer_authorized"),
    )
    .await;
    write_request(
        &mut stdin,
        transition_call(6, "embedded-stdio-human", "finish"),
    )
    .await;

    let initialized = read_response(&mut lines).await;
    let tools = read_response(&mut lines).await;
    let started = read_response(&mut lines).await;
    let stepped = read_response(&mut lines).await;
    let approved = read_response(&mut lines).await;
    let completed = read_response(&mut lines).await;

    assert_eq!(initialized["result"]["metadata"]["backend"], "embedded");
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 10);
    assert_eq!(structured(&started)["next_step_id"], "investigate");
    assert_eq!(
        structured(&stepped)["waiting_for_human"],
        json!(["engineer_authorized"])
    );
    assert_eq!(structured(&approved)["transitions"][0]["enabled"], true);
    assert_eq!(structured(&completed)["completed"], true);
    assert_eq!(structured(&completed)["current_state"], "COMPLETED");

    drop(stdin);
    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("embedded MCP process did not stop after stdin closed")
        .unwrap();
    assert!(status.success());
}

async fn send(server: &ChoreoMcpServer, request: Value) -> Value {
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("request must produce a response");
    serde_json::from_str(&response).unwrap()
}

fn jsonrpc(id: u64, method: &str, params: Option<Value>) -> Value {
    let mut request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
    });
    if let Some(params) = params {
        request["params"] = params;
    }
    request
}

fn run_ceremony_call(id: u64, ceremony_id: &str) -> Value {
    jsonrpc(
        id,
        "tools/call",
        Some(json!({
            "name": "choreo_run_ceremony",
            "arguments": {
                "ceremony_id": ceremony_id,
                "definition_yaml": CEREMONY_YAML,
                "context": { "requested_by": "codex-smoke" }
            }
        })),
    )
}

fn start_ceremony_call(id: u64, ceremony_id: &str) -> Value {
    tool_call(
        id,
        "choreo_start_ceremony",
        &json!({
            "ceremony_id": ceremony_id,
            "definition_yaml": HUMAN_APPROVAL_CEREMONY_YAML,
            "context": { "requested_by": "incremental-smoke" }
        }),
    )
}

fn start_collaborative_ceremony_call(id: u64, ceremony_id: &str) -> Value {
    tool_call(
        id,
        "choreo_start_ceremony",
        &json!({
            "ceremony_id": ceremony_id,
            "definition_yaml": COLLABORATIVE_TABLE_CEREMONY_YAML,
            "context": { "incident_ref": "INC-42" }
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn request_intervention_call(
    id: u64,
    ceremony_id: &str,
    intervention_id: &str,
    kind: &str,
    target_role_ids: Option<&[&str]>,
    message: &str,
    details: &Value,
    provenance: Option<&Value>,
) -> Value {
    let mut arguments = json!({
        "ceremony_id": ceremony_id,
        "intervention_id": intervention_id,
        "role_id": "ENGINEER",
        "kind": kind,
        "message": message,
        "details": details,
    });
    if let Some(target_role_ids) = target_role_ids {
        arguments["target_role_ids"] = json!(target_role_ids);
    }
    if let Some(provenance) = provenance {
        arguments["provenance"] = provenance.clone();
    }
    tool_call(id, "choreo_request_ceremony_intervention", &arguments)
}

fn respond_intervention_call(
    id: u64,
    ceremony_id: &str,
    intervention_id: &str,
    role_id: &str,
    message: &str,
    details: &Value,
) -> Value {
    tool_call(
        id,
        "choreo_respond_to_ceremony_intervention",
        &json!({
            "ceremony_id": ceremony_id,
            "intervention_id": intervention_id,
            "role_id": role_id,
            "message": message,
            "details": details,
        }),
    )
}

fn close_intervention_call(
    id: u64,
    ceremony_id: &str,
    intervention_id: &str,
    role_id: &str,
) -> Value {
    tool_call(
        id,
        "choreo_close_ceremony_intervention",
        &json!({
            "ceremony_id": ceremony_id,
            "intervention_id": intervention_id,
            "role_id": role_id,
        }),
    )
}

fn run_step_call(id: u64, ceremony_id: &str, step_id: &str) -> Value {
    tool_call(
        id,
        "choreo_run_ceremony_step",
        &json!({ "ceremony_id": ceremony_id, "step_id": step_id }),
    )
}

fn approval_call(id: u64, ceremony_id: &str, guard_name: &str) -> Value {
    tool_call(
        id,
        "choreo_approve_ceremony_guard",
        &json!({ "ceremony_id": ceremony_id, "guard_name": guard_name }),
    )
}

fn deferral_call(id: u64, ceremony_id: &str, guard_name: &str) -> Value {
    tool_call(
        id,
        "choreo_defer_ceremony_guard",
        &json!({
            "ceremony_id": ceremony_id,
            "guard_name": guard_name,
            "statement": "I do not know.",
            "reason": "The resolution is not clear.",
            "reconsider_when": ["New evidence explains the resolution."],
        }),
    )
}

fn transition_call(id: u64, ceremony_id: &str, trigger: &str) -> Value {
    tool_call(
        id,
        "choreo_apply_ceremony_transition",
        &json!({ "ceremony_id": ceremony_id, "trigger": trigger }),
    )
}

fn instance_call(id: u64, ceremony_id: &str) -> Value {
    tool_call(
        id,
        "choreo_get_ceremony_instance",
        &json!({ "ceremony_id": ceremony_id }),
    )
}

fn tool_call(id: u64, name: &str, arguments: &Value) -> Value {
    jsonrpc(
        id,
        "tools/call",
        Some(json!({ "name": name, "arguments": arguments })),
    )
}

async fn write_request(stdin: &mut tokio::process::ChildStdin, request: Value) {
    stdin
        .write_all(request.to_string().as_bytes())
        .await
        .unwrap();
    stdin.write_all(b"\n").await.unwrap();
    stdin.flush().await.unwrap();
}

async fn read_response(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
) -> Value {
    let line = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .expect("timed out waiting for MCP response")
        .unwrap()
        .expect("MCP process closed stdout unexpectedly");
    serde_json::from_str(&line).unwrap()
}

fn assert_completed(response: &Value) {
    let result = &response["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["completed"], true);
    assert_eq!(result["structuredContent"]["final_state"], "COMPLETED");
    assert_eq!(
        result["structuredContent"]["steps"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

fn structured(response: &Value) -> &Value {
    assert_eq!(response["result"]["isError"], false, "{response:?}");
    &response["result"]["structuredContent"]
}
