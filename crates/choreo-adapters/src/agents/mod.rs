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

#[cfg(feature = "agent-anthropic")]
pub mod anthropic;

#[cfg(feature = "agent-openai")]
pub mod openai;

// `agent-vllm` is declared in the crate manifest for forward
// compatibility but does not ship a module yet. Enabling it today
// is a no-op on this crate; a future slice adds the real module
// (likely as a thin reuse of the openai client, since vLLM speaks
// the same Chat Completions shape).
