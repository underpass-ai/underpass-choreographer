use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use choreo_tests_integration::grpc_fixture::GrpcFixture;

const SPRINT_PLANNING_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/sprint-planning.yaml");

/// Drives the four-role sprint planning end-to-end through RunCeremony.
/// Every turn after the priorities opening sets `see_prior: true`, so the
/// run threads priorities -> breakdown -> capacity -> commitment while
/// completing to the terminal state.
#[tokio::test]
async fn run_sprint_planning_executes_threaded_ceremony() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "integration-sprint-planning".to_owned(),
            definition_yaml: SPRINT_PLANNING_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    assert_eq!(response.definition_name, "sprint_planning");
    assert_eq!(response.final_state, "CLOSED");
    assert!(response.completed);
    assert_eq!(response.steps.len(), 4);
    assert!(response.steps.iter().all(|step| step.status == "COMPLETED"));

    for role in ["PRODUCT_OWNER", "ENGINEER", "SCRUM_MASTER", "DELIVERY_LEAD"] {
        assert!(
            response.steps.iter().any(|step| step.role_id == role),
            "missing completed role {role}"
        );
    }

    let diagram = &response.mermaid_sequence;
    for fragment in [
        "sequenceDiagram",
        "present_priorities [facilitation_prompt]",
        "task_breakdown [progress_prompt]",
        "capacity_review [challenge_prompt]",
        "scope_commitment [synthesis_prompt]",
        "scope_committed -> CLOSED",
    ] {
        assert!(
            diagram.contains(fragment),
            "diagram missing fragment `{fragment}`"
        );
    }
}
