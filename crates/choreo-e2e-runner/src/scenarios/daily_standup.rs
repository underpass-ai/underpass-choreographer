use anyhow::{bail, Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use tonic::transport::Channel;
use tracing::info;

const DAILY_STANDUP_CEREMONY: &str =
    include_str!("../../../../tests/e2e/ceremonies/daily-standup.yaml");

pub(crate) async fn verify_daily_standup_ceremony(
    client: &mut ChoreographerServiceClient<Channel>,
) -> Result<()> {
    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "e2e-daily-standup".to_owned(),
            definition_yaml: DAILY_STANDUP_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "e2e-runner".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .context("RunCeremony failed for daily standup ceremony")?
        .into_inner();

    if !response.completed {
        bail!(
            "expected daily standup to complete; final_state={}",
            response.final_state
        );
    }
    if response.final_state != "CLOSED" {
        bail!("expected final_state CLOSED, got {}", response.final_state);
    }
    if response.steps.len() != 4 {
        bail!(
            "expected 4 executed standup steps, got {}",
            response.steps.len()
        );
    }
    for role in ["SCRUM_MASTER", "ENGINEER", "QA_LEAD", "DELIVERY_LEAD"] {
        if !response
            .steps
            .iter()
            .any(|step| step.role_id == role && step.status == "COMPLETED")
        {
            bail!("daily standup response is missing completed role {role}");
        }
    }

    let diagram = &response.mermaid_sequence;
    for expected in [
        "sequenceDiagram",
        "open_standup [facilitation_prompt]",
        "progress_round [progress_prompt]",
        "impediment_review [challenge_prompt]",
        "commitment_summary [synthesis_prompt]",
        "commitments_made -> CLOSED",
    ] {
        if !diagram.contains(expected) {
            bail!("daily standup diagram does not contain expected fragment `{expected}`");
        }
    }

    info!(
        ceremony_id = response.ceremony_id,
        final_state = response.final_state,
        steps = response.steps.len(),
        diagram = %diagram,
        "daily standup ceremony executed and rendered"
    );
    Ok(())
}
