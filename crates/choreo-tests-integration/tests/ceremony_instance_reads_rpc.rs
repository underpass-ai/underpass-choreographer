//! The read side of a working session, over gRPC.
//!
//! These tests exist for one reason: the same question asked of the
//! server and of the embedded engine has to come back the same shape.
//! Both distributions now project through `CeremonyInstanceView`, and
//! the point of an integration test is to prove the wire carries what
//! the view holds — not to re-test the view.

use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    GetCeremonyInstanceRequest, ListCeremonyInstancesRequest, RunCeremonyRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use tonic::Code;

const EDITORIAL_MEETING_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/editorial-planning-meeting.yaml");

const CEREMONY_ID: &str = "integration-instance-reads";

#[tokio::test]
async fn a_finished_ceremony_can_be_read_back_over_grpc() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let run = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: CEREMONY_ID.to_owned(),
            definition_yaml: EDITORIAL_MEETING_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    let instance = client
        .get_ceremony_instance(GetCeremonyInstanceRequest {
            ceremony_id: CEREMONY_ID.to_owned(),
        })
        .await
        .expect("GetCeremonyInstance should succeed")
        .into_inner()
        .instance
        .expect("a run ceremony must be readable back");

    // What the run reported and what the instance holds are the same
    // facts reached two different ways.
    assert_eq!(instance.ceremony_id, CEREMONY_ID);
    assert_eq!(instance.definition_name, run.definition_name);
    assert_eq!(instance.definition_version, run.definition_version);
    assert_eq!(instance.current_state, run.final_state);
    assert_eq!(instance.completed, run.completed);
    assert_eq!(instance.steps.len(), run.steps.len());

    let mut projected: Vec<&str> = instance
        .steps
        .iter()
        .map(|step| step.step_id.as_str())
        .collect();
    let mut executed: Vec<&str> = run.steps.iter().map(|step| step.step_id.as_str()).collect();
    projected.sort_unstable();
    executed.sort_unstable();
    assert_eq!(projected, executed);

    // The projection renders a status the way the embedded engine does
    // — `StepStatus::as_label()`. `RunCeremonyResponse` predates it and
    // renders the same enum in SCREAMING_CASE through a mapper of its
    // own, so the two spellings are compared, not asserted equal. The
    // lowercase label is the one pinned here, because equality with the
    // embedded backend is what this surface promises.
    assert!(instance
        .steps
        .iter()
        .all(|step| step.status == "completed" && step.error.is_empty()));
    for (projected, executed) in instance.steps.iter().zip(run.steps.iter()) {
        assert_eq!(projected.status.to_uppercase(), executed.status);
        assert_eq!(projected.state_id, executed.state_id);
        assert_eq!(projected.attempt, executed.attempt);
    }

    // A closed session has nothing left to do and nobody left to wait
    // for. Asserting the emptiness matters as much as the content: it
    // is how a client decides not to show a call to action.
    assert!(instance.next_step_id.is_empty());
    assert!(instance.waiting_for_human.is_empty());
    assert!(instance.open_intervention_ids.is_empty());
    assert!(instance.transitions.is_empty());

    // RunCeremony supplies its own definition, so this instance is not
    // governed by the published catalogue and must not look like it is.
    assert!(instance.bound_definition_digest.is_empty());

    let listed = client
        .list_ceremony_instances(ListCeremonyInstancesRequest {})
        .await
        .expect("ListCeremonyInstances should succeed")
        .into_inner()
        .instances;

    assert_eq!(listed.len(), 1);
    // Both RPCs project through the same view; if they ever stop
    // agreeing, one of them grew a projection of its own.
    assert_eq!(listed[0], instance);
}

#[tokio::test]
async fn reading_a_ceremony_that_was_never_started_is_not_found() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let status = client
        .get_ceremony_instance(GetCeremonyInstanceRequest {
            ceremony_id: "never-started".to_owned(),
        })
        .await
        .expect_err("an unknown ceremony must not read as an empty one");

    assert_eq!(status.code(), Code::NotFound);
}

#[tokio::test]
async fn listing_before_anything_ran_is_empty_rather_than_an_error() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let listed = client
        .list_ceremony_instances(ListCeremonyInstancesRequest {})
        .await
        .expect("ListCeremonyInstances should succeed on an empty store")
        .into_inner()
        .instances;

    assert!(listed.is_empty());
}
