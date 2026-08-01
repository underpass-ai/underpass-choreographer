use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use choreo_tests_integration::grpc_fixture::GrpcFixture;

const TECHNICAL_DEBATE_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/technical-debate.yaml");

/// Drives the four-role technical debate end-to-end through RunCeremony.
/// The rebuttal runs `rounds: 2` and every turn after the thesis sets
/// `see_prior: true`, so the run exercises the threaded, multi-round path
/// while completing to the terminal state.
#[tokio::test]
async fn run_technical_debate_executes_threaded_ceremony() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "integration-technical-debate".to_owned(),
            definition_yaml: TECHNICAL_DEBATE_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    assert_eq!(response.definition_name, "technical_debate");
    assert_eq!(response.final_state, "ADJOURNED");
    assert!(response.completed);
    assert_eq!(response.steps.len(), 4);
    assert!(response.steps.iter().all(|step| step.status == "COMPLETED"));

    for role in ["PROPONENT", "OPPONENT", "REBUTTER", "ARBITER"] {
        assert!(
            response.steps.iter().any(|step| step.role_id == role),
            "missing completed role {role}"
        );
    }

    let diagram = &response.mermaid_sequence;
    for fragment in [
        "sequenceDiagram",
        "state_thesis [facilitation_prompt]",
        "raise_objections [challenge_prompt]",
        "answer_objections [progress_prompt]",
        "render_decision [synthesis_prompt]",
        "decision_rendered -> ADJOURNED",
    ] {
        assert!(
            diagram.contains(fragment),
            "diagram missing fragment `{fragment}`"
        );
    }
}
