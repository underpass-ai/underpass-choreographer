use anyhow::{bail, Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use tonic::transport::Channel;
use tracing::info;

const EDITORIAL_MEETING_CEREMONY: &str =
    include_str!("../../../../tests/e2e/ceremonies/editorial-planning-meeting.yaml");

pub(crate) async fn verify_editorial_meeting_ceremony_diagram(
    client: &mut ChoreographerServiceClient<Channel>,
) -> Result<()> {
    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "e2e-editorial-planning-meeting".to_owned(),
            definition_yaml: EDITORIAL_MEETING_CEREMONY.to_owned(),
            context: None,
            lease_owner_id: "e2e-runner".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .context("RunCeremony failed for editorial planning ceremony")?
        .into_inner();

    if !response.completed {
        bail!(
            "expected ceremony to complete; final_state={}",
            response.final_state
        );
    }
    if response.final_state != "CLOSED" {
        bail!("expected final_state CLOSED, got {}", response.final_state);
    }
    if response.steps.len() != 4 {
        bail!(
            "expected 4 executed ceremony steps, got {}",
            response.steps.len()
        );
    }
    for role in [
        "FACILITATOR",
        "CUSTOMER_ADVOCATE",
        "RISK_REVIEWER",
        "SYNTHESIZER",
    ] {
        if !response.steps.iter().any(|step| step.role_id == role) {
            bail!("RunCeremony response is missing role {role}");
        }
    }

    let diagram = &response.mermaid_sequence;
    for expected in [
        "sequenceDiagram",
        "open_room [facilitation_prompt]",
        "customer_story [persona_prompt]",
        "risk_check [challenge_prompt]",
        "decision_summary [synthesis_prompt]",
        "decision_written -> CLOSED",
    ] {
        if !diagram.contains(expected) {
            bail!("ceremony diagram does not contain expected fragment `{expected}`");
        }
    }

    info!(
        ceremony_id = response.ceremony_id,
        final_state = response.final_state,
        steps = response.steps.len(),
        diagram = %diagram,
        "editorial planning ceremony executed and rendered"
    );
    Ok(())
}
