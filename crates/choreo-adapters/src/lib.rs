//! Infrastructure adapters for the Underpass Choreographer.
//!
//! Implements ports defined in `choreo-core` using concrete technologies.
//! The Choreographer itself is **provider-agnostic**; every integration —
//! transport, message bus, or LLM vendor — is a peer adapter gated behind
//! its own Cargo feature flag. No provider is privileged.
//!
//! ## Always available
//!
//! | Adapter                             | Port                              |
//! |-------------------------------------|-----------------------------------|
//! | [`clock::SystemClock`]              | `ClockPort`                       |
//! | [`config::EnvConfiguration`]        | `ConfigurationPort`               |
//! | [`memory::InMemoryCouncilRegistry`] | `CouncilRegistryPort`             |
//! | [`memory::InMemoryDeliberationRepository`] | `DeliberationRepositoryPort` |
//! | [`memory::InMemoryAgentRegistry`]   | `AgentResolverPort` (+ writes)    |
//! | [`noop::NoopAgent`]                 | `AgentPort` (deterministic; tests / demos) |
//! | [`noop::NoopExecutor`]              | `ExecutorPort`                    |
//! | [`noop::NoopMessaging`]             | `MessagingPort`                   |
//! | [`scoring::UniformScoring`]         | `ScoringPort` (pass fraction)     |
//! | [`validators::ContentNonEmptyValidator`] | `ValidatorPort` (sanity check) |
//!
//! ## Feature-gated
//!
//! | Feature            | Integration                                 |
//! |--------------------|---------------------------------------------|
//! | `nats`             | NATS JetStream messaging adapter            |
//! | `postgres`         | Postgres deliberation repository (sqlx)     |
//! | `agent-vllm`       | vLLM / OpenAI-compatible local inference    |
//! | `agent-anthropic`  | Anthropic Messages API                      |
//! | `agent-openai`     | OpenAI Chat Completions / Responses API     |
//!
//! Additional provider adapters (frontier, local, rule-based, human-in-the-loop)
//! plug in through the same `AgentPort` trait with no core changes.

#![deny(missing_debug_implementations)]

pub mod clock;
pub mod config;
pub mod memory;
pub mod noop;
pub mod scoring;
pub mod validators;

#[cfg(feature = "grpc")]
pub mod grpc;

#[cfg(feature = "nats")]
pub mod nats;

#[cfg(feature = "postgres")]
pub mod postgres;

pub mod agents;
