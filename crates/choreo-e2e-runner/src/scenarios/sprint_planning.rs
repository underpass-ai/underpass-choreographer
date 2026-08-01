use anyhow::{bail, Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use tonic::transport::Channel;
use tracing::info;

const SPRINT_PLANNING_CEREMONY: &str =
    include_str!("../../../../tests/e2e/ceremonies/sprint-planning.yaml");

pub(crate) async fn verify_sprint_planning_ceremony(
    client: &mut ChoreographerServiceClient<Channel>,
) -> Result<()> {
    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "e2e-sprint-planning".to_owned(),
            definition_yaml: SPRINT_PLANNING_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "e2e-runner".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .context("RunCeremony failed for sprint planning ceremony")?
        .into_inner();

    if !response.completed {
        bail!(
            "expected sprint planning to complete; final_state={}",
            response.final_state
        );
    }
    if response.final_state != "CLOSED" {
        bail!("expected final_state CLOSED, got {}", response.final_state);
    }
    if response.steps.len() != 4 {
        bail!(
            "expected 4 executed planning steps, got {}",
            response.steps.len()
        );
    }
    for role in ["PRODUCT_OWNER", "ENGINEER", "SCRUM_MASTER", "DELIVERY_LEAD"] {
        if !response
            .steps
            .iter()
            .any(|step| step.role_id == role && step.status == "COMPLETED")
        {
            bail!("sprint planning response is missing completed role {role}");
        }
    }

    let diagram = &response.mermaid_sequence;
    for expected in [
        "sequenceDiagram",
        "present_priorities [facilitation_prompt]",
        "task_breakdown [progress_prompt]",
        "capacity_review [challenge_prompt]",
        "scope_commitment [synthesis_prompt]",
        "scope_committed -> CLOSED",
    ] {
        if !diagram.contains(expected) {
            bail!("sprint planning diagram does not contain expected fragment `{expected}`");
        }
    }

    info!(
        ceremony_id = response.ceremony_id,
        final_state = response.final_state,
        steps = response.steps.len(),
        diagram = %diagram,
        "sprint planning ceremony executed and rendered"
    );
    Ok(())
}
