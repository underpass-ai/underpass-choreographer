use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use choreo_tests_integration::grpc_fixture::GrpcFixture;

const EDITORIAL_MEETING_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/editorial-planning-meeting.yaml");

#[tokio::test]
async fn run_ceremony_executes_yaml_and_returns_trace_diagram() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "integration-editorial-meeting".to_owned(),
            definition_yaml: EDITORIAL_MEETING_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    assert_eq!(response.ceremony_id, "integration-editorial-meeting");
    assert_eq!(response.definition_name, "editorial_planning_meeting");
    assert_eq!(response.definition_version, "1.0");
    assert_eq!(response.final_state, "CLOSED");
    assert!(response.completed);
    assert_eq!(response.steps.len(), 4);
    assert!(response.steps.iter().all(|step| step.status == "COMPLETED"));
    assert!(response
        .steps
        .iter()
        .any(|step| step.role_id == "FACILITATOR"));
    assert!(response
        .steps
        .iter()
        .any(|step| step.role_id == "CUSTOMER_ADVOCATE"));
    assert!(response
        .steps
        .iter()
        .any(|step| step.role_id == "RISK_REVIEWER"));
    assert!(response
        .steps
        .iter()
        .any(|step| step.role_id == "SYNTHESIZER"));
    assert!(response.mermaid_sequence.contains("sequenceDiagram"));
    assert!(response
        .mermaid_sequence
        .contains("decision_summary [synthesis_prompt]"));
}
