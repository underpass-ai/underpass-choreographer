//! Agent provider adapters.
//!
//! Each provider is a peer adapter behind its own Cargo feature flag.
//! The Choreographer core is **provider-agnostic**: there is no
//! privileged vendor. Adding a new provider is always purely additive
//! — a new feature + a new module + a new `impl AgentPort`, no core
//! changes required.
//!
//! Secrets (API keys, bearer tokens, …) must never be printed through
//! `Debug` impls. Each provider adapter is expected to wrap its
//! credentials in an opaque type that masks the value on formatting.

// Shared infrastructure for provider adapters. `prompts` is reused
// by every provider that speaks natural language (so all current
// adapters); `openai_compat` is reused only by adapters that speak
// the Chat Completions wire shape (OpenAI + vLLM, not Anthropic).
//
// Both are `pub(super)` (i.e. visible only within `agents::*`).

#[cfg(any(
    feature = "agent-anthropic",
    feature = "agent-openai",
    feature = "agent-vllm"
))]
mod prompts;

#[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
mod openai_compat;

// Shared provider-endpoint validation (scheme allowlist, fail-fast).
// Available whenever any HTTP provider adapter is compiled in.
#[cfg(feature = "_http")]
mod endpoint;

// Shared latency + in-flight instrumentation for HTTP provider calls.
#[cfg(feature = "_http")]
mod instrument;

#[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
pub mod judge;

#[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
pub mod support_judge;

#[cfg(feature = "agent-anthropic")]
pub mod anthropic;

#[cfg(feature = "agent-openai")]
pub mod openai;

#[cfg(feature = "agent-vllm")]
pub mod vllm;

pub mod factory;
pub use factory::{
    DispatchingAgentFactory, ANTHROPIC_AGENT_KIND, OPENAI_AGENT_KIND, VLLM_AGENT_KIND,
};

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::{EvidenceSupportJudgePort, MetricsRecorderPort, ValidatorPort};

/// Build the LLM-judge validator from the environment, when enabled.
///
/// The judge is opt-in via `CHOREO_JUDGE_ENABLED` (`1`/`true`/`yes`). When
/// enabled it reuses the vLLM endpoint and model (`CHOREO_VLLM_ENDPOINT`,
/// `CHOREO_VLLM_MODEL`) and an optional `CHOREO_JUDGE_THRESHOLD` (default
/// `0.5`). The feature-gating lives here, mirroring
/// [`DispatchingAgentFactory::from_env`]: the composition root calls this
/// unconditionally and stays free of provider `cfg`s.
///
/// Returns `Ok(None)` when the judge is disabled, or when the binary was
/// built without a Chat-Completions provider feature (so the judge code is
/// not compiled in). Returns `Err` — failing fast — when the judge is
/// explicitly enabled but its endpoint/model/threshold are missing or
/// invalid.
// The Arc is consumed by the provider-backed branch. In a build without a
// compatible provider that branch is compiled out, so Clippy cannot observe
// the ownership transfer.
#[cfg_attr(
    not(any(feature = "agent-openai", feature = "agent-vllm")),
    allow(clippy::needless_pass_by_value)
)]
pub fn judge_from_env(
    metrics: Arc<dyn MetricsRecorderPort>,
) -> Result<Option<Arc<dyn ValidatorPort>>, DomainError> {
    if !judge_enabled() {
        return Ok(None);
    }
    #[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
    {
        build_env_judge(metrics).map(Some)
    }
    #[cfg(not(any(feature = "agent-openai", feature = "agent-vllm")))]
    {
        let _ = metrics;
        tracing::warn!(
            "CHOREO_JUDGE_ENABLED is set but this build has no Chat-Completions \
             provider feature; scoring stays uniform"
        );
        Ok(None)
    }
}

/// Whether the operator opted the judge in via `CHOREO_JUDGE_ENABLED`.
fn judge_enabled() -> bool {
    env_flag_enabled("CHOREO_JUDGE_ENABLED")
}

/// Whether an opt-in boolean env flag is set (`1`/`true`/`yes`).
fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    })
}

/// Build the evidence-support judge from the environment, when enabled.
///
/// The support judge is opt-in via `CHOREO_SUPPORT_JUDGE_ENABLED`
/// (`1`/`true`/`yes`) and reuses the vLLM endpoint and model
/// (`CHOREO_VLLM_ENDPOINT`, `CHOREO_VLLM_MODEL`), mirroring
/// [`judge_from_env`]. Note the asymmetry with the quality judge: the
/// support judge is *demanded per step* by an `output_contract`
/// declaring `evidence.semantic_support` — when a contract demands it
/// and this returns `None`, the `claims-evidence-supported` validator
/// fails the step loudly instead of running the gate voided.
///
/// Returns `Ok(None)` when disabled, or when the binary was built
/// without a Chat-Completions provider feature. Returns `Err` —
/// failing fast — when explicitly enabled but misconfigured.
// See `judge_from_env`: the no-provider feature set intentionally preserves
// the same ownership-shaped public API as provider-enabled builds.
#[cfg_attr(
    not(any(feature = "agent-openai", feature = "agent-vllm")),
    allow(clippy::needless_pass_by_value)
)]
pub fn support_judge_from_env(
    metrics: Arc<dyn MetricsRecorderPort>,
) -> Result<Option<Arc<dyn EvidenceSupportJudgePort>>, DomainError> {
    if !support_judge_enabled() {
        return Ok(None);
    }
    #[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
    {
        build_env_support_judge(metrics).map(Some)
    }
    #[cfg(not(any(feature = "agent-openai", feature = "agent-vllm")))]
    {
        let _ = metrics;
        tracing::warn!(
            "CHOREO_SUPPORT_JUDGE_ENABLED is set but this build has no Chat-Completions \
             provider feature; contracts demanding semantic support will fail their steps"
        );
        Ok(None)
    }
}

/// Whether the operator opted the support judge in via
/// `CHOREO_SUPPORT_JUDGE_ENABLED`.
fn support_judge_enabled() -> bool {
    env_flag_enabled("CHOREO_SUPPORT_JUDGE_ENABLED")
}

/// Construct the support judge from the vLLM endpoint/model env,
/// failing fast on a missing or invalid setting.
#[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
fn build_env_support_judge(
    metrics: Arc<dyn MetricsRecorderPort>,
) -> Result<Arc<dyn EvidenceSupportJudgePort>, DomainError> {
    let endpoint = std::env::var("CHOREO_VLLM_ENDPOINT").map_err(|_| DomainError::EmptyField {
        field: "support_judge.endpoint",
    })?;
    let model = std::env::var("CHOREO_VLLM_MODEL").map_err(|_| DomainError::EmptyField {
        field: "support_judge.model",
    })?;
    let judge = support_judge::LlmEvidenceSupportJudge::new(endpoint, model, metrics)?;
    Ok(Arc::new(judge))
}

/// Construct the judge from the vLLM endpoint/model env, failing fast on a
/// missing or invalid setting.
#[cfg(any(feature = "agent-openai", feature = "agent-vllm"))]
fn build_env_judge(
    metrics: Arc<dyn MetricsRecorderPort>,
) -> Result<Arc<dyn ValidatorPort>, DomainError> {
    let endpoint = std::env::var("CHOREO_VLLM_ENDPOINT").map_err(|_| DomainError::EmptyField {
        field: "judge.endpoint",
    })?;
    let model = std::env::var("CHOREO_VLLM_MODEL").map_err(|_| DomainError::EmptyField {
        field: "judge.model",
    })?;
    let threshold = std::env::var("CHOREO_JUDGE_THRESHOLD")
        .ok()
        .map_or(Ok(0.5), |value| {
            value
                .trim()
                .parse::<f64>()
                .map_err(|_| DomainError::EmptyField {
                    field: "judge.threshold",
                })
        })?;
    let judge = judge::LlmJudgeValidator::new(endpoint, model, threshold, metrics)?;
    Ok(Arc::new(judge))
}
