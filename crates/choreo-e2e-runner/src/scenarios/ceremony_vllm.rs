use anyhow::{bail, Context, Result};
use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use tonic::transport::Channel;
use tracing::info;

use super::ceremony_vllm_definition::CeremonyVllmDefinition;
use super::ceremony_vllm_provider_config::CeremonyVllmProviderConfig;

pub(crate) async fn verify_editorial_meeting_ceremony_against_vllm_kind(
    client: &mut ChoreographerServiceClient<Channel>,
) -> Result<()> {
    let provider = CeremonyVllmProviderConfig::from_env()?;
    let definition = CeremonyVllmDefinition::from_provider_config(&provider)?;

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "e2e-editorial-planning-meeting-vllm".to_owned(),
            definition_yaml: definition.as_yaml().to_owned(),
            context: None,
            lease_owner_id: "e2e-runner".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .context("RunCeremony failed for vLLM editorial planning ceremony")?
        .into_inner();

    if !response.completed {
        bail!(
            "expected vLLM ceremony to complete; final_state={}",
            response.final_state
        );
    }
    if response.final_state != "CLOSED" {
        bail!(
            "expected vLLM ceremony final_state CLOSED, got {}",
            response.final_state
        );
    }
    if response.steps.len() != 4 {
        bail!(
            "expected 4 executed vLLM ceremony steps, got {}",
            response.steps.len()
        );
    }
    for role in [
        "FACILITATOR",
        "CUSTOMER_ADVOCATE",
        "RISK_REVIEWER",
        "SYNTHESIZER",
    ] {
        if !response
            .steps
            .iter()
            .any(|step| step.role_id == role && step.status == "COMPLETED")
        {
            bail!("vLLM RunCeremony response is missing completed role {role}");
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
            bail!("vLLM ceremony diagram does not contain expected fragment `{expected}`");
        }
    }

    info!(
        ceremony_id = response.ceremony_id,
        final_state = response.final_state,
        steps = response.steps.len(),
        endpoint = provider.endpoint(),
        model = provider.model(),
        max_tokens = provider.max_tokens(),
        timeout_secs = provider.timeout_secs(),
        diagram = %diagram,
        "vLLM editorial planning ceremony executed and rendered"
    );
    Ok(())
}
