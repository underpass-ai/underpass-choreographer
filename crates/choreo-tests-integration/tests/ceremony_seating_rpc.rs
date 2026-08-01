//! Seating a working session over gRPC.
//!
//! A definition says what each role does. It does not say who plays
//! it, and the same ceremony run twice can seat different people. The
//! test that matters is not that the seating is recorded — it is that
//! the work then goes to whoever was seated.

use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    BindCeremonyParticipantsRequest, GetCeremonyInstanceRequest, RunCeremonyStepRequest,
    StartCeremonyRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use std::collections::HashMap;
use tonic::Code;

const SEATED_CEREMONY: &str = r#"
version: "1.0"
name: "seated_ceremony"
states:
  - id: OPEN
    initial: true
  - id: DONE
    terminal: true
transitions:
  - from: OPEN
    to: DONE
    trigger: finish
steps:
  - id: review
    state: OPEN
    handler: challenge_prompt
    config:
      participants:
        - risk_reviewer
      prompt: "Name the risks in this plan."
roles:
  - id: RISK_REVIEWER
    allowed_actions:
      - review
      - finish
"#;

async fn start(client: &mut ChoreographerServiceClient<tonic::transport::Channel>, id: &str) {
    client
        .start_ceremony(StartCeremonyRequest {
            ceremony_id: id.to_owned(),
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            definition_yaml: SEATED_CEREMONY.to_owned(),
            context: None,
        })
        .await
        .expect("StartCeremony should succeed");
}

#[tokio::test]
async fn a_seated_role_sends_its_work_to_the_panel_the_session_chose() {
    let fixture = GrpcFixture::start().await;

    // A panel that exists but that the definition never mentions.
    let seated = choreo_core::value_objects::Specialty::new("senior_sre_panel").unwrap();
    let agent_id = choreo_core::value_objects::AgentId::new("agent-senior-sre-0").unwrap();
    fixture
        .agents
        .register(std::sync::Arc::new(choreo_adapters::noop::NoopAgent::new(
            agent_id.clone(),
            seated.clone(),
        )))
        .await
        .expect("registering the panel's agent should succeed");
    fixture
        .councils
        .register(
            choreo_core::entities::Council::new(
                choreo_core::value_objects::CouncilId::new("council-senior-sre").unwrap(),
                seated.clone(),
                vec![agent_id],
                time::OffsetDateTime::now_utc(),
            )
            .unwrap(),
        )
        .await
        .expect("registering the panel should succeed");

    let mut client = ChoreographerServiceClient::new(fixture.channel.clone());
    let ceremony_id = "integration-seating";
    start(&mut client, ceremony_id).await;

    let seated_session = client
        .bind_ceremony_participants(BindCeremonyParticipantsRequest {
            ceremony_id: ceremony_id.to_owned(),
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            seating: HashMap::from([("RISK_REVIEWER".to_owned(), "senior_sre_panel".to_owned())]),
        })
        .await
        .expect("BindCeremonyParticipants should succeed")
        .into_inner()
        .instance
        .expect("seating must come back with the session");

    assert_eq!(seated_session.participant_bindings.len(), 1);
    let binding = &seated_session.participant_bindings[0];
    assert_eq!(binding.role_id, "RISK_REVIEWER");
    assert_eq!(binding.specialty, "senior_sre_panel");
    assert!(!binding.bound_at.is_empty());

    // The point of it all: the step declares `challenge_prompt`, and
    // the answer comes from the panel this session seated instead.
    let after_step = client
        .run_ceremony_step(RunCeremonyStepRequest {
            ceremony_id: ceremony_id.to_owned(),
            actor_kind: "agent".to_owned(),
            step_id: "review".to_owned(),
            lease_owner_id: "integration-test".to_owned(),
            idempotency_key: "integration-seating-review".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremonyStep should succeed")
        .into_inner()
        .instance
        .expect("a step that ran must come back with its session");

    let step = &after_step.steps[0];
    assert_eq!(step.status, "completed", "{}", step.error);
    let winner = step
        .output
        .as_ref()
        .and_then(|output| output.fields.get("winner_content"))
        .and_then(|value| value.kind.as_ref())
        .map(|kind| format!("{kind:?}"))
        .unwrap_or_default();
    assert!(
        winner.contains("agent-senior-sre-0"),
        "the work went somewhere other than the seated panel: {winner}"
    );
}

#[tokio::test]
async fn a_session_left_unseated_is_played_the_way_the_definition_says() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-unseated";
    start(&mut client, ceremony_id).await;

    let session = client
        .get_ceremony_instance(GetCeremonyInstanceRequest {
            ceremony_id: ceremony_id.to_owned(),
        })
        .await
        .expect("GetCeremonyInstance should succeed")
        .into_inner()
        .instance
        .expect("a started ceremony must be readable");

    // No seating is the usual case, not a lesser one, and the session
    // says so by carrying none rather than by inventing a default.
    assert!(session.participant_bindings.is_empty());

    let after_step = client
        .run_ceremony_step(RunCeremonyStepRequest {
            ceremony_id: ceremony_id.to_owned(),
            actor_kind: "agent".to_owned(),
            step_id: "review".to_owned(),
            lease_owner_id: "integration-test".to_owned(),
            idempotency_key: "integration-unseated-review".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("an unseated session still runs")
        .into_inner()
        .instance
        .expect("a step that ran must come back with its session");

    assert_eq!(after_step.steps[0].status, "completed");
}

#[tokio::test]
async fn a_seat_the_ceremony_never_declared_is_refused() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-unknown-seat";
    start(&mut client, ceremony_id).await;

    let status = client
        .bind_ceremony_participants(BindCeremonyParticipantsRequest {
            ceremony_id: ceremony_id.to_owned(),
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            seating: HashMap::from([("NOT_A_ROLE".to_owned(), "senior_sre_panel".to_owned())]),
        })
        .await
        .expect_err("a seat that does not exist cannot be filled");

    assert_eq!(status.code(), Code::NotFound);
}

#[tokio::test]
async fn seating_nobody_is_refused_rather_than_answered_with_done() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-empty-seating";
    start(&mut client, ceremony_id).await;

    let status = client
        .bind_ceremony_participants(BindCeremonyParticipantsRequest {
            ceremony_id: ceremony_id.to_owned(),
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            seating: HashMap::new(),
        })
        .await
        .expect_err("empty seating would change nothing");

    assert_ne!(status.code(), Code::Unknown);
}

#[tokio::test]
async fn a_panel_can_be_changed_halfway_through() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-reseated";
    start(&mut client, ceremony_id).await;

    for specialty in ["first_panel", "second_panel"] {
        client
            .bind_ceremony_participants(BindCeremonyParticipantsRequest {
                ceremony_id: ceremony_id.to_owned(),
                actor_id: "operator-1".to_owned(),
                actor_kind: "service".to_owned(),
                seating: HashMap::from([("RISK_REVIEWER".to_owned(), specialty.to_owned())]),
            })
            .await
            .expect("re-seating a session should succeed");
    }

    // A panel can become unavailable halfway through a working
    // session. A ceremony that could not be re-seated would have to be
    // abandoned and started again, losing everything already decided.
    let session = client
        .get_ceremony_instance(GetCeremonyInstanceRequest {
            ceremony_id: ceremony_id.to_owned(),
        })
        .await
        .expect("GetCeremonyInstance should succeed")
        .into_inner()
        .instance
        .expect("a started ceremony must be readable");

    assert_eq!(session.participant_bindings.len(), 1);
    assert_eq!(session.participant_bindings[0].specialty, "second_panel");
}
