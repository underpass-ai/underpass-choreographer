//! Domain ports.
//!
//! Ports are narrow, segregated traits. Each one names exactly one
//! responsibility that the application layer requires from the outside
//! world (agents, message bus, clock, persistence, …). Adapters in
//! `choreo-adapters` implement these traits.
//!
//! Hexagonal discipline:
//!
//! - Dependency direction is **adapters → app → core**. Ports live in
//!   core and import nothing from app or adapters.
//! - All ports return [`crate::DomainError`] so the application layer
//!   never leaks adapter-shaped errors (I/O, wire, parsing) upward.
//! - Segregation follows ISP: no port has more than one reason to
//!   change.

mod agent;
mod agent_factory;
mod agent_registry;
mod agent_resolver;
mod audit_journal;
mod ceremony_context_store;
mod ceremony_definition_repository;
mod ceremony_definition_source;
mod ceremony_evidence_request;
mod ceremony_evidence_source;
mod ceremony_instance_repository;
mod ceremony_step_handler;
mod ceremony_step_handler_request;
mod clock;
mod configuration;
mod contract_registry;
mod council_registry;
mod deliberation_observer;
mod deliberation_repository;
mod evidence_support_judge;
mod executor;
mod messaging;
mod metrics_recorder;
mod noop_ceremony_context_store;
mod scoring;
mod statistics;
mod validator;

pub use agent::{AgentPort, Critique, DraftRequest, Revision};
pub use agent_factory::{AgentDescriptor, AgentFactoryPort};
pub use agent_registry::AgentRegistryPort;
pub use agent_resolver::AgentResolverPort;
pub use audit_journal::AuditJournalPort;
pub use ceremony_context_store::CeremonyContextStorePort;
pub use ceremony_definition_repository::CeremonyDefinitionRepositoryPort;
pub use ceremony_definition_source::CeremonyDefinitionSourcePort;
pub use ceremony_evidence_request::CeremonyEvidenceRequest;
pub use ceremony_evidence_source::CeremonyEvidenceSourcePort;
pub use ceremony_instance_repository::CeremonyInstanceRepositoryPort;
pub use ceremony_step_handler::CeremonyStepHandlerPort;
pub use ceremony_step_handler_request::CeremonyStepHandlerRequest;
pub use clock::ClockPort;
pub use configuration::{ConfigurationPort, GrpcTlsConfig, ServiceConfig};
pub use contract_registry::ContractRegistryPort;
pub use council_registry::CouncilRegistryPort;
pub use deliberation_observer::{DeliberationObserverPort, NullObserver};
pub use deliberation_repository::DeliberationRepositoryPort;
pub use evidence_support_judge::{EvidenceExcerpt, EvidenceSupportJudgePort, SupportVerdict};
pub use executor::{ExecutionOutcome, ExecutorPort};
pub use messaging::{DomainEvent, MessagingPort, SubscriptionHandler};
pub use metrics_recorder::{MetricsRecorderPort, NoopMetricsRecorder};
pub use noop_ceremony_context_store::NoopCeremonyContextStore;
pub use scoring::ScoringPort;
pub use statistics::StatisticsPort;
pub use validator::ValidatorPort;
