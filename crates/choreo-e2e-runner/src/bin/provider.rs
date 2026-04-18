//! Provider-level E2E runner.
//!
//! Puts the `agent-vllm` adapter in front of a real vLLM endpoint
//! (mTLS or plain) and exercises the full `AgentPort` surface
//! against it: `generate`, `critique`, `revise`, plus a sanity check
//! that all three calls returned non-trivial text.
//!
//! Exits 0 on success, non-zero on the first failed assertion. The
//! Kubernetes Job manifest under `tests/e2e/kubernetes/` mounts the
//! `e2e-client-tls` secret and wires these env vars:
//!
//!   CHOREO_VLLM_ENDPOINT         (required) — e.g. https://.../
//!   CHOREO_VLLM_MODEL            (required) — e.g. Qwen/Qwen3.5-9B
//!   CHOREO_VLLM_CLIENT_CERT_PATH (optional) — PEM file
//!   CHOREO_VLLM_CLIENT_KEY_PATH  (optional) — PEM file
//!   CHOREO_VLLM_BEARER_TOKEN     (optional) — bearer if no mTLS
//!   CHOREO_VLLM_MAX_TOKENS       (optional) — default 1024
//!
//! The runner does **not** start a choreographer. Its job is to pin
//! the adapter-to-vLLM contract. Full end-to-end through the
//! choreographer (with the vLLM agent registered into a council)
//! is covered by a future experiment.

use std::env;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use choreo_adapters::agents::vllm::{VllmAgent, VllmBearerToken, VllmClientIdentity, VllmConfig};
use choreo_core::entities::TaskConstraints;
use choreo_core::ports::{AgentPort, Critique, DraftRequest};
use choreo_core::value_objects::{AgentId, NumAgents, Rounds, Rubric, Specialty, TaskDescription};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
#[allow(clippy::too_many_lines)] // linear E2E script; splitting it fragments the assertion story
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .compact()
        .init();

    let endpoint = require_env("CHOREO_VLLM_ENDPOINT")?;
    let model = require_env("CHOREO_VLLM_MODEL")?;
    let max_tokens = env::var("CHOREO_VLLM_MAX_TOKENS")
        .ok()
        .map(|raw| raw.parse::<u32>())
        .transpose()
        .context("CHOREO_VLLM_MAX_TOKENS must parse as u32")?
        .unwrap_or(1024);

    let mut config = VllmConfig::new(&model)
        .context("VllmConfig::new rejected the model name")?
        .with_endpoint(&endpoint)
        .context("VllmConfig::with_endpoint rejected the endpoint URL")?
        .with_max_tokens(max_tokens)
        .context("VllmConfig rejected max_tokens")?
        .with_timeout(Duration::from_secs(300));

    // mTLS: both the cert and key paths must be set together. If one
    // is set and the other isn't, fail loudly — it's almost
    // certainly a misconfigured env.
    match (
        env::var("CHOREO_VLLM_CLIENT_CERT_PATH").ok(),
        env::var("CHOREO_VLLM_CLIENT_KEY_PATH").ok(),
    ) {
        (Some(cert), Some(key)) => {
            let cert_pem = std::fs::read(&cert)
                .with_context(|| format!("reading client cert PEM from {cert}"))?;
            let key_pem = std::fs::read(&key)
                .with_context(|| format!("reading client key PEM from {key}"))?;
            let identity = VllmClientIdentity::from_cert_and_key(&cert_pem, &key_pem)
                .map_err(|err| anyhow!("invalid client identity: {err}"))?;
            config = config.with_client_identity(identity);
            info!(
                cert = %Path::new(&cert).display(),
                key = %Path::new(&key).display(),
                "mtls client identity loaded"
            );
        }
        (None, None) => {}
        (cert, key) => bail!(
            "mTLS misconfigured: CHOREO_VLLM_CLIENT_CERT_PATH={:?}, CHOREO_VLLM_CLIENT_KEY_PATH={:?} (both required or neither)",
            cert.is_some(),
            key.is_some(),
        ),
    }

    if let Some(token) = env::var("CHOREO_VLLM_BEARER_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let bearer =
            VllmBearerToken::new(token).map_err(|err| anyhow!("invalid bearer token: {err}"))?;
        config = config.with_bearer(bearer);
        info!("bearer token loaded");
    }

    let agent = VllmAgent::new(
        AgentId::new("e2e-vllm-agent").unwrap(),
        Specialty::new("triage").unwrap(),
        config,
    )
    .map_err(|err| anyhow!("VllmAgent::new failed: {err}"))?;

    let constraints = TaskConstraints::new(
        Rubric::empty(),
        Rounds::new(0).unwrap(),
        Some(NumAgents::new(1).unwrap()),
        None,
    );
    let task = TaskDescription::new(
        "Investigate a p1 payment latency spike. Suggest two hypotheses and one next step.",
    )
    .unwrap();

    info!(endpoint, model, "scenario 1/3: generate");
    let draft = agent
        .generate(DraftRequest {
            task: task.clone(),
            constraints: constraints.clone(),
            diverse: true,
        })
        .await
        .context("agent.generate failed")?;
    require_nonempty("generate.content", &draft.content, 20)?;
    info!(
        chars = draft.content.len(),
        head = %truncate(&draft.content, 80),
        "generate ok"
    );

    info!("scenario 2/3: critique");
    let critique = agent
        .critique(&draft.content, &constraints)
        .await
        .context("agent.critique failed")?;
    require_nonempty("critique.feedback", &critique.feedback, 20)?;
    info!(
        chars = critique.feedback.len(),
        head = %truncate(&critique.feedback, 80),
        "critique ok"
    );

    info!("scenario 3/3: revise");
    let revised = agent
        .revise(
            &draft.content,
            &Critique {
                feedback: critique.feedback.clone(),
            },
        )
        .await
        .context("agent.revise failed")?;
    require_nonempty("revise.content", &revised.content, 20)?;
    if revised.content.trim() == draft.content.trim() {
        warn!(
            "revise returned the same content as generate — the model may not have actually revised"
        );
    }
    info!(
        chars = revised.content.len(),
        head = %truncate(&revised.content, 80),
        "revise ok"
    );

    info!("all provider-E2E scenarios passed");
    Ok(())
}

fn require_env(name: &str) -> Result<String> {
    let raw = env::var(name).map_err(|_| anyhow!("required env var {name} is not set"))?;
    if raw.trim().is_empty() {
        bail!("required env var {name} is empty");
    }
    Ok(raw)
}

fn require_nonempty(field: &str, s: &str, min_chars: usize) -> Result<()> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        bail!("{field} is empty");
    }
    if trimmed.chars().count() < min_chars {
        bail!(
            "{field} is suspiciously short ({} chars, want >= {min_chars}): {:?}",
            trimmed.chars().count(),
            trimmed
        );
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_owned()
    } else {
        let head: String = trimmed.chars().take(max).collect();
        format!("{head}…")
    }
}
