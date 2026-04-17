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

#[cfg(feature = "agent-anthropic")]
pub mod anthropic;

#[cfg(feature = "agent-openai")]
pub mod openai;

#[cfg(feature = "agent-vllm")]
pub mod vllm;
