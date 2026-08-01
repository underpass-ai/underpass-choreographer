//! One MCP tool, two backends, one answer.
//!
//! This is the test the whole alignment effort exists for. A client
//! asks `choreo_get_ceremony_instance` and must not be able to tell
//! whether the engine was in its own process or across a network.
//!
//! Values cannot match — timestamps, identifiers and agent output all
//! differ between two independently-run sessions — so what is compared
//! is the **shape**: every key at every level, and the kind of every
//! leaf. A field present on one side and absent on the other, or a
//! string where the other says null, is exactly the drift that made
//! this worth pinning.

use std::collections::BTreeSet;

use choreo_embedded::EmbeddedChoreographer;
use choreo_mcp::backend::{ChoreoMcpGrpcTlsConfig, ChoreoMcpToolBackend};
use choreo_mcp::{EmbeddedChoreoMcpBackend, GrpcChoreoMcpBackend};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    ApplyCeremonyTransitionRequest, DeferCeremonyGuardRequest, RequestCeremonyInterventionRequest,
    RespondToCeremonyInterventionRequest, RunCeremonyStepRequest, StartCeremonyRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use serde_json::{json, Value};

const CEREMONY_ID: &str = "parity-session";

/// Rich on purpose. Empty collections hide shape differences, so the
/// session this drives has a step that ran, a transition that fired,
/// a guard that was deferred and an agenda item that was answered.
const PARITY_CEREMONY: &str = r#"
version: "1.0"
name: "parity_ceremony"
states:
  - id: OPEN
    initial: true
  - id: REVIEW
  - id: DONE
    terminal: true
transitions:
  - from: OPEN
    to: REVIEW
    trigger: opened
    guards:
      - work_done
  - from: REVIEW
    to: DONE
    trigger: approve
    guards:
      - human_approved
guards:
  work_done:
    type: automated
    check: "step_status:work:COMPLETED"
  human_approved:
    type: human
    check: manual_approval
steps:
  - id: work
    state: OPEN
    handler: facilitation_prompt
    config:
      participants:
        - facilitator
      prompt: "Say whether the work is done."
roles:
  - id: FACILITATOR
    allowed_actions:
      - work
      - opened
      - approve
      - request_intervention
      - respond_to_intervention
"#;

/// Fields the contract declares free-form. Their *presence* is part
/// of the shape; their contents are whatever the ceremony put there,
/// and the two sessions ran different step handlers, so descending
/// into them would compare the handlers rather than the contract.
const OPEN_ENDED: [&str; 4] = ["output", "details", "context", "evidence_pack"];

fn is_open_ended(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .is_some_and(|last| OPEN_ENDED.contains(&last))
}

/// The set of keys at every path, plus the kind of every leaf. Two
/// answers of the same shape produce the same set.
fn shape(value: &Value, path: &str, into: &mut BTreeSet<String>) {
    if is_open_ended(path) {
        return;
    }
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                let child_path = format!("{path}.{key}");
                into.insert(child_path.clone());
                shape(child, &child_path, into);
            }
        }
        Value::Array(items) => {
            // Only the first element: a shape is per-element, and two
            // sessions need not have produced the same number of them.
            if let Some(first) = items.first() {
                let child_path = format!("{path}[]");
                into.insert(child_path.clone());
                shape(first, &child_path, into);
            }
        }
        leaf => {
            into.insert(format!("{path} :: {}", kind_of(leaf)));
        }
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn structured(result: &Value) -> Value {
    result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| panic!("a tool result should carry structured content: {result:?}"))
}

/// Drive the same session on the server, over its own RPCs.
async fn build_remote_session(fixture: &GrpcFixture) {
    let mut client = ChoreographerServiceClient::new(fixture.channel.clone());

    client
        .start_ceremony(StartCeremonyRequest {
            ceremony_id: CEREMONY_ID.to_owned(),
            definition_yaml: PARITY_CEREMONY.to_owned(),
            context: None,
        })
        .await
        .expect("StartCeremony should succeed");
    client
        .run_ceremony_step(RunCeremonyStepRequest {
            ceremony_id: CEREMONY_ID.to_owned(),
            step_id: "work".to_owned(),
            lease_owner_id: "parity".to_owned(),
            idempotency_key: "parity-work".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremonyStep should succeed");
    client
        .apply_ceremony_transition(ApplyCeremonyTransitionRequest {
            actor_kind: "agent".to_owned(),
            ceremony_id: CEREMONY_ID.to_owned(),
            trigger: "opened".to_owned(),
        })
        .await
        .expect("ApplyCeremonyTransition should succeed");
    client
        .defer_ceremony_guard(DeferCeremonyGuardRequest {
            role_kind: "human".to_owned(),
            role_id: "FACILITATOR".to_owned(),
            ceremony_id: CEREMONY_ID.to_owned(),
            guard_name: "human_approved".to_owned(),
            statement: "Not yet.".to_owned(),
            reason: "The reviewer is out.".to_owned(),
            reconsider_when: vec!["the reviewer is back".to_owned()],
        })
        .await
        .expect("DeferCeremonyGuard should succeed");
    client
        .request_ceremony_intervention(RequestCeremonyInterventionRequest {
            ceremony_id: CEREMONY_ID.to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "FACILITATOR".to_owned(),
            kind: "opinion".to_owned(),
            target_role_ids: Vec::new(),
            message: "Does anyone object?".to_owned(),
            details: None,
            provenance: None,
        })
        .await
        .expect("RequestCeremonyIntervention should succeed");
    client
        .respond_to_ceremony_intervention(RespondToCeremonyInterventionRequest {
            ceremony_id: CEREMONY_ID.to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "FACILITATOR".to_owned(),
            message: "No objection.".to_owned(),
            details: None,
        })
        .await
        .expect("RespondToCeremonyIntervention should succeed");
}

/// The same session again, in this process, through the MCP tools the
/// in-process backend already exposes.
async fn build_embedded_session(backend: &EmbeddedChoreoMcpBackend) {
    let call = |name: &'static str, args: Value| async move {
        backend
            .call_tool(name, &args)
            .await
            .unwrap_or_else(|error| panic!("{name} should succeed in-process: {error}"))
    };

    call(
        "choreo_start_ceremony",
        json!({ "ceremony_id": CEREMONY_ID, "definition_yaml": PARITY_CEREMONY }),
    )
    .await;
    call(
        "choreo_run_ceremony_step",
        json!({ "ceremony_id": CEREMONY_ID, "step_id": "work" }),
    )
    .await;
    call(
        "choreo_apply_ceremony_transition",
        json!({ "ceremony_id": CEREMONY_ID, "trigger": "opened", "actor_kind": "agent" }),
    )
    .await;
    call(
        "choreo_defer_ceremony_guard",
        json!({
            "ceremony_id": CEREMONY_ID,
            "role_id": "FACILITATOR", "role_kind": "human", "guard_name": "human_approved",
            "statement": "Not yet.",
            "reason": "The reviewer is out.",
            "reconsider_when": ["the reviewer is back"],
        }),
    )
    .await;
    call(
        "choreo_request_ceremony_intervention",
        json!({
            "ceremony_id": CEREMONY_ID,
            "intervention_id": "item-1",
            "role_id": "FACILITATOR",
            "kind": "opinion",
            "message": "Does anyone object?",
        }),
    )
    .await;
    call(
        "choreo_respond_to_ceremony_intervention",
        json!({
            "ceremony_id": CEREMONY_ID,
            "intervention_id": "item-1",
            "role_id": "FACILITATOR",
            "message": "No objection.",
        }),
    )
    .await;
}

#[tokio::test]
async fn one_tool_answers_the_same_shape_whichever_backend_served_it() {
    let fixture = GrpcFixture::start().await;
    build_remote_session(&fixture).await;

    let embedded = EmbeddedChoreoMcpBackend::new(EmbeddedChoreographer::default());
    build_embedded_session(&embedded).await;

    let arguments = json!({ "ceremony_id": CEREMONY_ID });
    let remote = GrpcChoreoMcpBackend::new(
        format!("http://{}", fixture.addr),
        ChoreoMcpGrpcTlsConfig::disabled(),
    );

    let over_the_wire = structured(
        &remote
            .call_tool("choreo_get_ceremony_instance", &arguments)
            .await
            .expect("the gRPC backend should answer"),
    );
    let in_process = structured(
        &embedded
            .call_tool("choreo_get_ceremony_instance", &arguments)
            .await
            .expect("the in-process backend should answer"),
    );

    // Sanity: the session really is as rich as it was meant to be, so
    // an accidentally-empty collection cannot make the shapes agree by
    // having nothing to disagree about.
    assert_eq!(in_process["steps"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        in_process["guard_deferrals"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        in_process["interventions"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        in_process["interventions"][0]["responses"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let mut wire_shape = BTreeSet::new();
    shape(&over_the_wire, "", &mut wire_shape);
    let mut process_shape = BTreeSet::new();
    shape(&in_process, "", &mut process_shape);

    let only_on_the_wire = wire_shape
        .difference(&process_shape)
        .cloned()
        .collect::<Vec<_>>();
    let only_in_process = process_shape
        .difference(&wire_shape)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        only_on_the_wire.is_empty() && only_in_process.is_empty(),
        "the same tool answered two different shapes\n  only over the wire: {only_on_the_wire:#?}\n  only in process: {only_in_process:#?}"
    );
}

#[tokio::test]
async fn both_backends_advertise_every_ceremony_tool() {
    let embedded = EmbeddedChoreoMcpBackend::new(EmbeddedChoreographer::default());
    let remote =
        GrpcChoreoMcpBackend::new("http://127.0.0.1:1", ChoreoMcpGrpcTlsConfig::disabled());

    // Every verb of a working session, on both sides. A tool served
    // by one backend and not the other is a client that works until
    // it is pointed at the other engine.
    for tool in [
        "choreo_get_ceremony_instance",
        "choreo_list_ceremony_instances",
        "choreo_start_ceremony",
        "choreo_start_published_ceremony",
        "choreo_run_ceremony_step",
        "choreo_apply_ceremony_transition",
        "choreo_approve_ceremony_guard",
        "choreo_defer_ceremony_guard",
        "choreo_request_ceremony_intervention",
        "choreo_respond_to_ceremony_intervention",
        "choreo_close_ceremony_intervention",
        "choreo_collect_ceremony_evidence",
        "choreo_validate_ceremony_draft",
        "choreo_explain_ceremony_draft",
        "choreo_publish_ceremony_definition",
        "choreo_diff_ceremony_definitions",
        "choreo_bind_ceremony_participants",
    ] {
        assert!(
            embedded.supports_tool(tool),
            "{tool} should be served in process"
        );
        assert!(
            remote.supports_tool(tool),
            "{tool} should be served over gRPC"
        );
    }
}

/// Driving the session through the *tools* on both sides, not through
/// raw RPCs on one. This is the shape a client actually meets.
#[tokio::test]
async fn the_same_tool_calls_drive_both_backends_to_the_same_shape() {
    let fixture = GrpcFixture::start().await;
    let remote = GrpcChoreoMcpBackend::new(
        format!("http://{}", fixture.addr),
        ChoreoMcpGrpcTlsConfig::disabled(),
    );
    let embedded = EmbeddedChoreoMcpBackend::new(EmbeddedChoreographer::default());

    for backend in [
        &remote as &dyn ChoreoMcpToolBackend,
        &embedded as &dyn ChoreoMcpToolBackend,
    ] {
        let name = backend.backend_name();
        for (tool, args) in [
            (
                "choreo_start_ceremony",
                json!({ "ceremony_id": "tools-parity", "definition_yaml": PARITY_CEREMONY }),
            ),
            (
                "choreo_run_ceremony_step",
                json!({ "ceremony_id": "tools-parity", "step_id": "work" }),
            ),
            (
                "choreo_apply_ceremony_transition",
                json!({ "ceremony_id": "tools-parity", "trigger": "opened", "actor_kind": "agent" }),
            ),
            (
                "choreo_request_ceremony_intervention",
                json!({
                    "ceremony_id": "tools-parity",
                    "intervention_id": "item-1",
                    "role_id": "FACILITATOR",
                    "kind": "opinion",
                    "message": "Does anyone object?",
                }),
            ),
        ] {
            backend
                .call_tool(tool, &args)
                .await
                .unwrap_or_else(|error| panic!("{tool} failed on the {name} backend: {error}"));
        }
    }

    let arguments = json!({ "ceremony_id": "tools-parity" });
    let over_the_wire = structured(
        &remote
            .call_tool("choreo_get_ceremony_instance", &arguments)
            .await
            .expect("the gRPC backend should answer"),
    );
    let in_process = structured(
        &embedded
            .call_tool("choreo_get_ceremony_instance", &arguments)
            .await
            .expect("the in-process backend should answer"),
    );

    let mut wire_shape = BTreeSet::new();
    shape(&over_the_wire, "", &mut wire_shape);
    let mut process_shape = BTreeSet::new();
    shape(&in_process, "", &mut process_shape);

    assert_eq!(
        wire_shape, process_shape,
        "the same sequence of tool calls left two different shapes"
    );
}
