//! The part of a working session a person does, over gRPC.
//!
//! A guard that only a human can satisfy is what turns a ceremony
//! from something that runs into something someone takes part in.
//! These tests drive that from a remote client: the session reports
//! it is waiting for a person, the person approves or defers, and an
//! agenda item is opened, answered and closed.

use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    ApplyCeremonyTransitionRequest, ApproveCeremonyGuardRequest, CeremonyInstanceState,
    CloseCeremonyInterventionRequest, CollectCeremonyEvidenceRequest, DeferCeremonyGuardRequest,
    GetCeremonyInstanceRequest, RequestCeremonyInterventionRequest,
    RespondToCeremonyInterventionRequest, StartCeremonyRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use tonic::transport::Channel;
use tonic::Code;

/// A session whose only way forward is a person saying yes.
const HUMAN_GUARD_CEREMONY: &str = r#"
version: "1.0"
name: "remote_human_guard"
states:
  - id: WAITING
    initial: true
  - id: APPROVED
    terminal: true
transitions:
  - from: WAITING
    to: APPROVED
    trigger: approve
    guards:
      - human_approved
guards:
  human_approved:
    type: human
    check: manual_approval
roles:
  - id: APPROVER
    allowed_actions:
      - approve
      - request_intervention
      - respond_to_intervention
"#;

async fn start(
    client: &mut ChoreographerServiceClient<Channel>,
    ceremony_id: &str,
) -> CeremonyInstanceState {
    client
        .start_ceremony(StartCeremonyRequest {
            ceremony_id: ceremony_id.to_owned(),
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            definition_yaml: HUMAN_GUARD_CEREMONY.to_owned(),
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
async fn a_session_waiting_on_a_person_says_so_and_moves_once_they_approve() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-human-approve";

    let started = start(&mut client, ceremony_id).await;
    // This is the field a client turns into "waiting for you".
    assert_eq!(started.waiting_for_human, vec!["human_approved".to_owned()]);
    assert!(!started.completed);

    let approved = client
        .approve_ceremony_guard(ApproveCeremonyGuardRequest {
            role_kind: "human".to_owned(),
            role_id: "APPROVER".to_owned(),
            ceremony_id: ceremony_id.to_owned(),
            guard_name: "human_approved".to_owned(),
        })
        .await
        .expect("ApproveCeremonyGuard should succeed")
        .into_inner()
        .instance
        .expect("approving must come back with the session");

    // Nobody is waited on any more, and the move it was blocking is
    // now offered as enabled.
    assert!(approved.waiting_for_human.is_empty());
    let out = approved
        .transitions
        .iter()
        .find(|transition| transition.trigger == "approve")
        .expect("the guarded transition should be listed");
    assert!(out.enabled);
    assert!(out.guards.iter().all(|guard| guard.satisfied));
    assert_eq!(read(&mut client, ceremony_id).await, approved);

    let closed = client
        .apply_ceremony_transition(ApplyCeremonyTransitionRequest {
            actor_kind: "agent".to_owned(),
            ceremony_id: ceremony_id.to_owned(),
            trigger: "approve".to_owned(),
        })
        .await
        .expect("the approved transition should fire")
        .into_inner()
        .instance
        .expect("a transition that fired must come back with its session");

    assert_eq!(closed.current_state, "APPROVED");
    assert!(closed.completed);
}

#[tokio::test]
async fn deferring_records_the_decision_instead_of_leaving_silence() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-human-defer";

    start(&mut client, ceremony_id).await;

    let deferred = client
        .defer_ceremony_guard(DeferCeremonyGuardRequest {
            role_kind: "human".to_owned(),
            role_id: "APPROVER".to_owned(),
            ceremony_id: ceremony_id.to_owned(),
            guard_name: "human_approved".to_owned(),
            statement: "Not approving today.".to_owned(),
            reason: "The rollback path is untested.".to_owned(),
            reconsider_when: vec!["the rollback has been rehearsed".to_owned()],
        })
        .await
        .expect("DeferCeremonyGuard should succeed")
        .into_inner()
        .instance
        .expect("deferring must come back with the session");

    let deferral = deferred
        .guard_deferrals
        .iter()
        .find(|deferral| deferral.guard_name == "human_approved")
        .expect("the deferral should be on the session");
    assert_eq!(deferral.statement, "Not approving today.");
    assert_eq!(deferral.reason, "The rollback path is untested.");
    assert_eq!(
        deferral.reconsider_when,
        vec!["the rollback has been rehearsed".to_owned()]
    );

    // A deferral is a decision, not an approval: the session has not
    // moved, and it is no longer reported as waiting on a person who
    // has in fact already answered.
    assert_eq!(deferred.current_state, "WAITING");
    assert!(!deferred.completed);
    let out = deferred
        .transitions
        .iter()
        .find(|transition| transition.trigger == "approve")
        .expect("the guarded transition should still be listed");
    assert!(!out.enabled);
    assert_eq!(read(&mut client, ceremony_id).await, deferred);
}

#[tokio::test]
async fn an_agenda_item_is_opened_answered_and_closed() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-intervention";

    start(&mut client, ceremony_id).await;

    let opened = client
        .request_ceremony_intervention(RequestCeremonyInterventionRequest {
            ceremony_id: ceremony_id.to_owned(),
            role_kind: "human".to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "APPROVER".to_owned(),
            kind: "investigation".to_owned(),
            target_role_ids: Vec::new(),
            message: "Which rollback did we rehearse last?".to_owned(),
            details: None,
            provenance: None,
        })
        .await
        .expect("RequestCeremonyIntervention should succeed")
        .into_inner()
        .instance
        .expect("opening an item must come back with the session");

    assert_eq!(opened.open_intervention_ids, vec!["item-1".to_owned()]);
    let item = opened
        .interventions
        .iter()
        .find(|intervention| intervention.intervention_id == "item-1")
        .expect("the item should be on the session");
    assert_eq!(item.kind, "investigation");
    assert_eq!(item.status, "open");
    assert!(item.responses.is_empty());

    let answered = client
        .respond_to_ceremony_intervention(RespondToCeremonyInterventionRequest {
            ceremony_id: ceremony_id.to_owned(),
            role_kind: "human".to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "APPROVER".to_owned(),
            message: "The one from the March release.".to_owned(),
            details: None,
        })
        .await
        .expect("RespondToCeremonyIntervention should succeed")
        .into_inner()
        .instance
        .expect("answering must come back with the session");

    let item = answered
        .interventions
        .iter()
        .find(|intervention| intervention.intervention_id == "item-1")
        .expect("the item should still be on the session");
    assert_eq!(item.responses.len(), 1);
    assert_eq!(item.responses[0].role_id, "APPROVER");
    // Answered is not the same as settled: it stays open until someone
    // closes it, which is the difference between a reply and a decision.
    assert_eq!(item.status, "open");
    assert_eq!(answered.open_intervention_ids, vec!["item-1".to_owned()]);

    let settled = client
        .close_ceremony_intervention(CloseCeremonyInterventionRequest {
            ceremony_id: ceremony_id.to_owned(),
            role_kind: "human".to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "APPROVER".to_owned(),
        })
        .await
        .expect("CloseCeremonyIntervention should succeed")
        .into_inner()
        .instance
        .expect("closing must come back with the session");

    assert!(settled.open_intervention_ids.is_empty());
    assert_eq!(
        settled
            .interventions
            .iter()
            .find(|intervention| intervention.intervention_id == "item-1")
            .expect("a closed item stays on the record")
            .status,
        "closed"
    );
    assert_eq!(read(&mut client, ceremony_id).await, settled);
}

#[tokio::test]
async fn collecting_evidence_says_there_is_no_source_rather_than_inventing_one() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-evidence";

    start(&mut client, ceremony_id).await;
    client
        .request_ceremony_intervention(RequestCeremonyInterventionRequest {
            ceremony_id: ceremony_id.to_owned(),
            role_kind: "human".to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "APPROVER".to_owned(),
            kind: "investigation".to_owned(),
            target_role_ids: Vec::new(),
            message: "What is in the dead-letter queue?".to_owned(),
            details: None,
            provenance: None,
        })
        .await
        .expect("RequestCeremonyIntervention should succeed");

    // The server ships with no evidence source. The honest answer is
    // that there is none — not a fabricated pack, and not a missing
    // method that leaves the caller guessing.
    let status = client
        .collect_ceremony_evidence(CollectCeremonyEvidenceRequest {
            ceremony_id: ceremony_id.to_owned(),
            role_kind: "agent".to_owned(),
            intervention_id: "item-1".to_owned(),
            role_id: "APPROVER".to_owned(),
            source_id: "dead-letter-queue".to_owned(),
            query: "count by subject".to_owned(),
            details: None,
        })
        .await
        .expect_err("no evidence source is wired, so this cannot succeed");

    assert_eq!(status.code(), Code::NotFound);

    // And the failed attempt left the session as it was.
    let after = read(&mut client, ceremony_id).await;
    assert_eq!(after.open_intervention_ids, vec!["item-1".to_owned()]);
}

#[tokio::test]
async fn a_guard_that_does_not_exist_is_refused() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);
    let ceremony_id = "integration-unknown-guard";

    start(&mut client, ceremony_id).await;

    let status = client
        .approve_ceremony_guard(ApproveCeremonyGuardRequest {
            role_kind: "human".to_owned(),
            role_id: "APPROVER".to_owned(),
            ceremony_id: ceremony_id.to_owned(),
            guard_name: "not_a_guard".to_owned(),
        })
        .await
        .expect_err("approving a guard the ceremony does not declare must fail");

    assert_ne!(status.code(), Code::Unknown);
    assert_eq!(
        read(&mut client, ceremony_id).await.waiting_for_human,
        vec!["human_approved".to_owned()]
    );
}
