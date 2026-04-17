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

// `agent-openai` and `agent-vllm` features are declared in the crate
// manifest for forward compatibility but do not ship a module yet.
// Enabling those features today is a no-op on this crate; future
// slices add the real modules without re-introducing the Cargo-level
// boilerplate.
