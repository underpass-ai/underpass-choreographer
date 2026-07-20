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

#[tokio::test]
async fn embedded_server_advertises_only_executable_tools() {
    let server = ChoreoMcpServer::embedded();

    let initialized = send(&server, jsonrpc(1, "initialize", None)).await;
    assert_eq!(initialized["result"]["metadata"]["backend"], "embedded");

    let tools = send(&server, jsonrpc(2, "tools/list", None)).await;
    let catalog = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0]["name"], "choreo_run_ceremony");

    let completed = send(&server, run_ceremony_call(3, "embedded-direct-smoke")).await;
    assert_completed(&completed);
}

#[tokio::test]
async fn embedded_binary_completes_mcp_stdio_handshake_and_tool_call() {
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
    write_request(&mut stdin, run_ceremony_call(3, "embedded-stdio-smoke")).await;

    let initialized = read_response(&mut lines).await;
    let tools = read_response(&mut lines).await;
    let completed = read_response(&mut lines).await;

    assert_eq!(initialized["result"]["metadata"]["backend"], "embedded");
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 1);
    assert_completed(&completed);

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
