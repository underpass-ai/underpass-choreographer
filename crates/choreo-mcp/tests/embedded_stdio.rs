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
            "choreo_apply_ceremony_transition",
            "choreo_get_ceremony_instance",
        ]
    );

    let completed = send(&server, run_ceremony_call(3, "embedded-direct-smoke")).await;
    assert_completed(&completed);
}

#[tokio::test]
async fn embedded_server_pauses_until_a_human_guard_is_approved() {
    let server = ChoreoMcpServer::embedded();
    let ceremony_id = "embedded-human-approval";

    let started = send(&server, start_ceremony_call(1, ceremony_id)).await;
    assert_eq!(structured(&started)["current_state"], "INVESTIGATING");
    assert_eq!(structured(&started)["next_step_id"], "investigate");

    let stepped = send(&server, run_step_call(2, ceremony_id, "investigate")).await;
    assert!(structured(&stepped)["next_step_id"].is_null());
    assert_eq!(
        structured(&stepped)["waiting_for_human"],
        json!(["engineer_authorized"])
    );

    let blocked = send(&server, transition_call(3, ceremony_id, "finish")).await;
    assert_eq!(blocked["result"]["isError"], true);

    let inspected = send(&server, instance_call(4, ceremony_id)).await;
    assert_eq!(
        structured(&inspected)["waiting_for_human"],
        json!(["engineer_authorized"])
    );

    let rejected = send(
        &server,
        approval_call(5, ceremony_id, "investigation_completed"),
    )
    .await;
    assert_eq!(rejected["result"]["isError"], true);

    let approved = send(
        &server,
        approval_call(6, ceremony_id, "engineer_authorized"),
    )
    .await;
    assert_eq!(structured(&approved)["waiting_for_human"], json!([]));
    assert_eq!(structured(&approved)["transitions"][0]["enabled"], true);

    let completed = send(&server, transition_call(7, ceremony_id, "finish")).await;
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
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 6);
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
