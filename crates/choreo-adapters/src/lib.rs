//! Infrastructure adapters for the Underpass Choreographer.
//!
//! Implements ports defined in `choreo-core` using concrete technologies.
//! The Choreographer itself is **provider-agnostic**; every integration —
//! transport, message bus, or LLM vendor — is a peer adapter gated behind
//! its own Cargo feature flag. No provider is privileged.
//!
//! ## Available feature-gated adapters
//!
//! | Feature            | Integration                                 |
//! |--------------------|---------------------------------------------|
//! | `nats`             | NATS JetStream messaging adapter            |
//! | `agent-vllm`       | vLLM / OpenAI-compatible local inference    |
//! | `agent-anthropic`  | Anthropic Messages API                      |
//! | `agent-openai`     | OpenAI Chat Completions / Responses API     |
//!
//! Additional provider adapters (frontier, local, rule-based, human-in-the-loop)
//! plug in through the same `AgentPort` trait with no core changes.

pub mod config {}

#[cfg(feature = "nats")]
pub mod nats {}

pub mod agents {
    #[cfg(feature = "agent-vllm")]
    pub mod vllm {}

    #[cfg(feature = "agent-anthropic")]
    pub mod anthropic {}

    #[cfg(feature = "agent-openai")]
    pub mod openai {}
}
