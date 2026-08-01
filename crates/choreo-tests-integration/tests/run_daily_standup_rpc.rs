use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use choreo_tests_integration::grpc_fixture::GrpcFixture;

const DAILY_STANDUP_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/daily-standup.yaml");

/// Drives the four-role daily standup end-to-end through the RunCeremony
/// RPC. The fixture wires the context store, so the run exercises the
/// transcript-threading path (`see_prior: true` on every turn after the
/// opening) while completing to the terminal state.
#[tokio::test]
async fn run_daily_standup_executes_threaded_ceremony() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "integration-daily-standup".to_owned(),
            definition_yaml: DAILY_STANDUP_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    assert_eq!(response.ceremony_id, "integration-daily-standup");
    assert_eq!(response.definition_name, "daily_standup");
    assert_eq!(response.final_state, "CLOSED");
    assert!(response.completed);
    assert_eq!(response.steps.len(), 4);
    assert!(response.steps.iter().all(|step| step.status == "COMPLETED"));

    for role in ["SCRUM_MASTER", "ENGINEER", "QA_LEAD", "DELIVERY_LEAD"] {
        assert!(
            response.steps.iter().any(|step| step.role_id == role),
            "missing completed role {role}"
        );
    }

    let diagram = &response.mermaid_sequence;
    assert!(diagram.contains("sequenceDiagram"));
    for fragment in [
        "open_standup [facilitation_prompt]",
        "progress_round [progress_prompt]",
        "impediment_review [challenge_prompt]",
        "commitment_summary [synthesis_prompt]",
        "commitments_made -> CLOSED",
    ] {
        assert!(
            diagram.contains(fragment),
            "diagram missing fragment `{fragment}`"
        );
    }
}
