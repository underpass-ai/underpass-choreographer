use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use choreo_tests_integration::grpc_fixture::GrpcFixture;

const SPEAKER_TALK_QA_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/speaker-talk-qa.yaml");

/// Drives the four-role speaker talk + Q&A end-to-end through RunCeremony.
/// The audience questions and the speaker's answers set `see_prior: true`,
/// so questions and answers are grounded in the actual talk while the run
/// completes to the terminal state.
#[tokio::test]
async fn run_speaker_talk_qa_executes_threaded_ceremony() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "integration-speaker-talk-qa".to_owned(),
            definition_yaml: SPEAKER_TALK_QA_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    assert_eq!(response.definition_name, "speaker_talk_qa");
    assert_eq!(response.final_state, "CLOSED");
    assert!(response.completed);
    assert_eq!(response.steps.len(), 4);
    assert!(response.steps.iter().all(|step| step.status == "COMPLETED"));

    for role in ["SPEAKER", "AUDIENCE", "SPEAKER_RESPONDENT", "MODERATOR"] {
        assert!(
            response.steps.iter().any(|step| step.role_id == role),
            "missing completed role {role}"
        );
    }

    let diagram = &response.mermaid_sequence;
    for fragment in [
        "sequenceDiagram",
        "deliver_talk [facilitation_prompt]",
        "audience_questions [challenge_prompt]",
        "speaker_answers [progress_prompt]",
        "takeaway_synthesis [synthesis_prompt]",
        "takeaways_captured -> CLOSED",
    ] {
        assert!(
            diagram.contains(fragment),
            "diagram missing fragment `{fragment}`"
        );
    }
}
