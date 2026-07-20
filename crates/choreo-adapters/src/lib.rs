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
//! | [`metrics::PrometheusMetricsRecorder`] | `MetricsRecorderPort`          |
//! | [`noop::NoopAgent`]                 | `AgentPort` (deterministic; tests / demos) |
//! | [`noop::NoopExecutor`]              | `ExecutorPort`                    |
//! | [`noop::NoopMessaging`]             | `MessagingPort`                   |
//! | [`scoring::UniformScoring`]         | `ScoringPort` (pass fraction)     |
//! | [`scoring::JudgeAwareScoring`]      | `ScoringPort` (judge verdict, wired when a judge is configured) |
//! | [`validators::ContentNonEmptyValidator`] | `ValidatorPort` (sanity check) |
//!
//! ## Feature-gated
//!
//! | Feature            | Integration                                 |
//! |--------------------|---------------------------------------------|
//! | `grpc` (default)   | Tonic gRPC server adapter (`grpc::*`)       |
//! | `nats`             | NATS JetStream messaging adapter            |
//! | `runtime-grpc`     | Outbound Underpass Runtime gRPC executor    |
//! | `postgres`         | Postgres deliberation repository (sqlx)     |
//! | `otel`             | gRPC W3C tracecontext → OpenTelemetry bridge |
//! | `agent-vllm`       | vLLM / OpenAI-compatible local inference    |
//! | `agent-anthropic`  | Anthropic Messages API                      |
//! | `agent-openai`     | OpenAI Chat Completions / Responses API     |
//!
//! Additional provider adapters (frontier, local, rule-based, human-in-the-loop)
//! plug in through the same `AgentPort` trait with no core changes.

#![deny(missing_debug_implementations)]

pub mod ceremony;
pub mod clock;
pub mod config;
pub mod memory;
pub mod mermaid;
pub mod metrics;
pub mod noop;
#[cfg(feature = "runtime-grpc")]
pub mod runtime;
pub mod scoring;
pub mod validators;
pub mod yaml;

#[cfg(feature = "grpc")]
pub mod grpc;

/// Re-exports intended only for this crate's integration tests.
///
/// Hidden from rustdoc because no production consumer should depend
/// on these items — they exist so the integration harness under
/// `tests/` can reach helpers that are otherwise module-private.
#[cfg(all(feature = "grpc", feature = "otel"))]
#[doc(hidden)]
pub mod __test_only {
    pub use crate::grpc::tracecontext::link_span_to_metadata;
}

#[cfg(feature = "nats")]
pub mod nats;

#[cfg(feature = "postgres")]
pub mod postgres;

pub mod agents;
