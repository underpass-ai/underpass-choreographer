//! Driving a ceremony move by move over gRPC.
//!
//! `RunCeremony` runs a whole session in one call, which only works
//! for a session that needs nobody. These RPCs are the other case: a
//! client that stops between moves, looks at what happened, and then
//! decides. The test walks a real ceremony to its close one call at a
//! time and checks the engine says the same thing at every point it
//! is asked, whichever RPC does the asking.

use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_core::entities::{PublicationOutcome, PublishedCeremonyDefinition};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    ApplyCeremonyTransitionRequest, CeremonyInstanceState, GetCeremonyInstanceRequest,
    RunCeremonyStepRequest, StartCeremonyRequest, StartPublishedCeremonyRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use tonic::transport::Channel;
use tonic::Code;

const EDITORIAL_MEETING_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/editorial-planning-meeting.yaml");

/// Every step paired with the trigger that leaves the state it sits in.
const MOVES: [(&str, &str); 4] = [
    ("open_room", "context_shared"),
    ("customer_story", "options_collected"),
    ("risk_check", "risks_reviewed"),
    ("decision_summary", "decision_written"),
];

async fn start(
    client: &mut ChoreographerServiceClient<Channel>,
    ceremony_id: &str,
) -> CeremonyInstanceState {
    client
        .start_ceremony(StartCeremonyRequest {
            ceremony_id: ceremony_id.to_owned(),
            definition_yaml: EDITORIAL_MEETING_CEREMONY.to_owned(),
            context: None,
        })
        .await
        .expect("StartCeremony should succeed")
        .into_inner()
        .instance
        .expect("a started ceremony must come back")
}

async fn read(
    client: &mut ChoreographerServiceClient<Channel>,
    ceremony_id: &str,
) -> CeremonyInstanceState {
    client
        .get_ceremony_instance(GetCeremonyInstanceRequest {
            ceremony_id: ceremony_id.to_owned(),
        })
        .await
        .expect("GetCeremonyInstance should succeed")
        .into_inner()
        .instance
        .expect("a started ceremony must be readable")
}

#[tokio::test]
async fn a_ceremony_can_be_walked_to_its_close_one_call_at_a_time() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-lifecycle";

    let started = start(&mut client, ceremony_id).await;
    assert_eq!(started.ceremony_id, ceremony_id);
    assert_eq!(started.definition_name, "editorial_planning_meeting");
    assert_eq!(started.current_state, "OPENING");
    assert!(!started.completed);
    assert_eq!(started.next_step_id, "open_room");
    // The way out of OPENING is listed but not enabled: a client is
    // told what the move would be and why it cannot make it yet.
    let out_of_opening = started
        .transitions
        .iter()
        .find(|transition| transition.trigger == "context_shared")
        .expect("the transition leaving OPENING should be visible from the start");
    assert!(!out_of_opening.enabled);
    assert!(out_of_opening
        .guards
        .iter()
        .any(|guard| guard.name == "open_room_completed" && !guard.satisfied));
    // A YAML-supplied definition is not governed by the catalogue.
    assert!(started.bound_definition_digest.is_empty());

    for (index, (step_id, trigger)) in MOVES.iter().enumerate() {
        let after_step = client
            .run_ceremony_step(RunCeremonyStepRequest {
                ceremony_id: ceremony_id.to_owned(),
                step_id: (*step_id).to_owned(),
                lease_owner_id: "integration-test".to_owned(),
                idempotency_key: format!("integration-lifecycle-{step_id}"),
                lease_ttl_ms: 60_000,
            })
            .await
            .unwrap_or_else(|status| panic!("RunCeremonyStep({step_id}) failed: {status}"))
            .into_inner()
            .instance
            .expect("a step that ran must come back with its session");

        // The step is done and the engine already knows the trigger
        // leaving this state is now enabled — that is what lets a
        // client decide its next move without a second call.
        let ran = after_step
            .steps
            .iter()
            .find(|step| step.step_id == *step_id)
            .unwrap_or_else(|| panic!("step {step_id} missing from the session"));
        assert_eq!(ran.status, "completed", "{step_id}: {}", ran.error);
        let enabled = after_step
            .transitions
            .iter()
            .find(|transition| transition.trigger == *trigger)
            .unwrap_or_else(|| panic!("{trigger} should be listed once {step_id} has run"));
        assert!(
            enabled.enabled,
            "{trigger} should be enabled once {step_id} has run"
        );
        assert!(enabled.guards.iter().all(|guard| guard.satisfied));

        // What the mutation answered and what a read answers are the
        // same value. If these ever diverge, one of the two paths grew
        // a projection of its own.
        assert_eq!(read(&mut client, ceremony_id).await, after_step);

        let after_transition = client
            .apply_ceremony_transition(ApplyCeremonyTransitionRequest {
                ceremony_id: ceremony_id.to_owned(),
                trigger: (*trigger).to_owned(),
            })
            .await
            .unwrap_or_else(|status| panic!("ApplyCeremonyTransition({trigger}) failed: {status}"))
            .into_inner()
            .instance
            .expect("a transition that fired must come back with its session");

        let last_move = index + 1 == MOVES.len();
        if last_move {
            assert_eq!(after_transition.current_state, "CLOSED");
            assert!(after_transition.completed);
            assert!(after_transition.next_step_id.is_empty());
            assert!(after_transition.transitions.is_empty());
        } else {
            assert!(!after_transition.completed);
            assert_eq!(after_transition.next_step_id, MOVES[index + 1].0);
        }
        assert_eq!(read(&mut client, ceremony_id).await, after_transition);
    }

    let closed = read(&mut client, ceremony_id).await;
    assert!(closed.steps.iter().all(|step| step.status == "completed"));
    assert!(closed.waiting_for_human.is_empty());
}

#[tokio::test]
async fn a_transition_whose_guard_is_unmet_does_not_fire() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-lifecycle-guarded";

    let started = start(&mut client, ceremony_id).await;
    // Nothing has run, so the guard on the way out of OPENING is unmet
    // and the engine must not report the move as available.
    assert!(started
        .transitions
        .iter()
        .all(|transition| !transition.enabled));

    let status = client
        .apply_ceremony_transition(ApplyCeremonyTransitionRequest {
            ceremony_id: ceremony_id.to_owned(),
            trigger: "context_shared".to_owned(),
        })
        .await
        .expect_err("a guarded transition must not fire before its guard is satisfied");
    assert_ne!(status.code(), Code::Unknown);

    // And the refusal left the session where it was.
    let after = read(&mut client, ceremony_id).await;
    assert_eq!(after.current_state, "OPENING");
    assert_eq!(after.next_step_id, "open_room");
}

#[tokio::test]
async fn a_step_that_belongs_to_another_state_is_refused() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-lifecycle-out-of-order";

    start(&mut client, ceremony_id).await;

    let status = client
        .run_ceremony_step(RunCeremonyStepRequest {
            ceremony_id: ceremony_id.to_owned(),
            // Belongs to SYNTHESIZING; the session is in OPENING.
            step_id: "decision_summary".to_owned(),
            lease_owner_id: "integration-test".to_owned(),
            idempotency_key: "integration-out-of-order".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect_err("a step outside the current state must not run");
    assert_ne!(status.code(), Code::Unknown);

    let after = read(&mut client, ceremony_id).await;
    assert_eq!(after.current_state, "OPENING");
    assert!(after.steps.iter().all(|step| step.status != "completed"));
}

#[tokio::test]
async fn a_published_ceremony_is_bound_to_its_digest_and_can_be_advanced() {
    let fixture = GrpcFixture::start().await;
    let definition = CeremonyDefinitionYaml::parse_str(EDITORIAL_MEETING_CEREMONY)
        .expect("the fixture ceremony should parse");
    let published = PublishedCeremonyDefinition::seal(definition).expect("sealing should succeed");
    let digest = published.digest();
    assert!(matches!(
        fixture
            .ceremony_publications
            .publish(published)
            .await
            .expect("publishing should succeed"),
        PublicationOutcome::Published(_)
    ));

    let mut client = ChoreographerServiceClient::new(fixture.channel.clone());
    let ceremony_id = "integration-published";
    let started = client
        .start_published_ceremony(StartPublishedCeremonyRequest {
            ceremony_id: ceremony_id.to_owned(),
            ceremony: "editorial_planning_meeting".to_owned(),
            version: "1.0".to_owned(),
            context: None,
        })
        .await
        .expect("StartPublishedCeremony should succeed")
        .into_inner()
        .instance
        .expect("a started ceremony must come back");

    // Unlike a session started from supplied YAML, this one records
    // which published definition it runs — the whole reason to start
    // from the catalogue rather than from a document in a request.
    assert_eq!(started.bound_definition_digest, digest.to_string());
    assert_eq!(started.current_state, "OPENING");
    assert_eq!(started.next_step_id, "open_room");

    // And it is advanced by exactly the same calls as any other. The
    // definition it runs lives only in the catalogue, so this only
    // works because the step resolves from the session's binding
    // rather than from the definition repository.
    let after_step = client
        .run_ceremony_step(RunCeremonyStepRequest {
            ceremony_id: ceremony_id.to_owned(),
            step_id: "open_room".to_owned(),
            lease_owner_id: "integration-test".to_owned(),
            idempotency_key: "integration-published-open-room".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("a published ceremony must be advanceable")
        .into_inner()
        .instance
        .expect("a step that ran must come back with its session");

    assert_eq!(
        after_step.bound_definition_digest,
        digest.to_string(),
        "advancing must not quietly unbind the session"
    );
    assert_eq!(read(&mut client, ceremony_id).await, after_step);
}
