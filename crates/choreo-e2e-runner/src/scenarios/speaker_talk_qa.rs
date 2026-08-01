use anyhow::{bail, Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use tonic::transport::Channel;
use tracing::info;

const SPEAKER_TALK_QA_CEREMONY: &str =
    include_str!("../../../../tests/e2e/ceremonies/speaker-talk-qa.yaml");

pub(crate) async fn verify_speaker_talk_qa_ceremony(
    client: &mut ChoreographerServiceClient<Channel>,
) -> Result<()> {
    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "e2e-speaker-talk-qa".to_owned(),
            definition_yaml: SPEAKER_TALK_QA_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "e2e-runner".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .context("RunCeremony failed for speaker talk + Q&A ceremony")?
        .into_inner();

    if !response.completed {
        bail!(
            "expected speaker talk + Q&A to complete; final_state={}",
            response.final_state
        );
    }
    if response.final_state != "CLOSED" {
        bail!("expected final_state CLOSED, got {}", response.final_state);
    }
    if response.steps.len() != 4 {
        bail!(
            "expected 4 executed talk steps, got {}",
            response.steps.len()
        );
    }
    for role in ["SPEAKER", "AUDIENCE", "SPEAKER_RESPONDENT", "MODERATOR"] {
        if !response
            .steps
            .iter()
            .any(|step| step.role_id == role && step.status == "COMPLETED")
        {
            bail!("speaker talk + Q&A response is missing completed role {role}");
        }
    }

    let diagram = &response.mermaid_sequence;
    for expected in [
        "sequenceDiagram",
        "deliver_talk [facilitation_prompt]",
        "audience_questions [challenge_prompt]",
        "speaker_answers [progress_prompt]",
        "takeaway_synthesis [synthesis_prompt]",
        "takeaways_captured -> CLOSED",
    ] {
        if !diagram.contains(expected) {
            bail!("speaker talk + Q&A diagram does not contain expected fragment `{expected}`");
        }
    }

    info!(
        ceremony_id = response.ceremony_id,
        final_state = response.final_state,
        steps = response.steps.len(),
        diagram = %diagram,
        "speaker talk + Q&A ceremony executed and rendered"
    );
    Ok(())
}
