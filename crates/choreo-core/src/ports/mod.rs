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
mod ceremony_definition_publication;
mod ceremony_definition_repository;
mod ceremony_definition_source;
mod ceremony_evidence_request;
mod ceremony_evidence_source;
mod ceremony_instance_repository;
mod ceremony_step_handler;
mod ceremony_step_handler_request;
mod ceremony_transcript_store;
mod ceremony_unit_of_work;
mod clock;
mod configuration;
mod contract_registry;
mod council_registry;
mod deliberation_observer;
mod deliberation_repository;
mod evidence_support_judge;
mod executor;
mod memory_reader;
mod memory_writer;
mod messaging;
mod metrics_recorder;
mod noop_ceremony_transcript_store;
mod outbox;
mod scoring;
mod statistics;
mod validator;

pub use agent::{AgentPort, Critique, DraftRequest, Revision};
pub use agent_factory::{AgentDescriptor, AgentFactoryPort};
pub use agent_registry::AgentRegistryPort;
pub use agent_resolver::AgentResolverPort;
pub use audit_journal::AuditJournalPort;
pub use ceremony_transcript_store::CeremonyTranscriptStorePort;

pub use ceremony_definition_publication::CeremonyDefinitionPublicationPort;
pub use ceremony_definition_repository::CeremonyDefinitionRepositoryPort;
pub use ceremony_definition_source::CeremonyDefinitionSourcePort;
pub use ceremony_evidence_request::CeremonyEvidenceRequest;
pub use ceremony_evidence_source::CeremonyEvidenceSourcePort;
pub use ceremony_instance_repository::CeremonyInstanceRepositoryPort;
pub use ceremony_step_handler::CeremonyStepHandlerPort;
pub use ceremony_step_handler_request::CeremonyStepHandlerRequest;
/// Former name of [`CeremonyTranscriptStorePort`].
///
/// Kept so a host can move at its own pace rather than in lockstep
/// with this repository. Due for removal before the first public tag —
/// a compatibility alias that outlives its migration is just a second
/// name for the same thing.
#[deprecated(
    since = "0.1.0",
    note = "renamed to CeremonyTranscriptStorePort: the port appends and replays a transcript, nothing more"
)]
pub use ceremony_transcript_store::CeremonyTranscriptStorePort as CeremonyContextStorePort;
pub use ceremony_unit_of_work::CeremonyUnitOfWorkPort;
pub use clock::ClockPort;
pub use configuration::{ConfigurationPort, GrpcTlsConfig, ServiceConfig};
pub use contract_registry::ContractRegistryPort;
pub use council_registry::CouncilRegistryPort;
pub use deliberation_observer::{DeliberationObserverPort, NullObserver};
pub use deliberation_repository::DeliberationRepositoryPort;
pub use evidence_support_judge::{EvidenceExcerpt, EvidenceSupportJudgePort, SupportVerdict};
pub use executor::{ExecutionOutcome, ExecutorPort};
pub use memory_reader::{MemoryReaderPort, MemoryRecollection};
pub use memory_writer::{MemoryWriteOutcome, MemoryWriterPort};
pub use messaging::{DomainEvent, MessagingPort, SubscriptionHandler};
pub use metrics_recorder::{MetricsRecorderPort, NoopMetricsRecorder};
pub use noop_ceremony_transcript_store::NoopCeremonyTranscriptStore;
pub use outbox::{OutboxPort, OutboxTransportPort};
pub use scoring::ScoringPort;
pub use statistics::StatisticsPort;
pub use validator::ValidatorPort;
