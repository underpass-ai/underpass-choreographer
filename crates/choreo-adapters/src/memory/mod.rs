//! In-memory adapters backing the domain ports that hold state.
//!
//! These are the simplest possible implementations: data lives in a
//! mutex-guarded map inside an [`Arc`]. They are safe to share across
//! tasks and are production-safe for single-replica deployments; for
//! multi-replica use cases, swap them for a persistent adapter.

mod agent_registry;
mod audit_journal;
mod ceremony_context_store;
mod ceremony_definition_repository;
mod ceremony_instance_repository;
mod ceremony_store;
mod contract_registry;
mod council_registry;
mod deliberation_repository;
mod statistics;

pub use agent_registry::InMemoryAgentRegistry;
pub use audit_journal::InMemoryAuditJournal;
pub use ceremony_context_store::InMemoryCeremonyContextStore;
pub use ceremony_definition_repository::InMemoryCeremonyDefinitionRepository;
pub use ceremony_instance_repository::InMemoryCeremonyInstanceRepository;
pub use ceremony_store::InMemoryCeremonyStore;
pub use contract_registry::InMemoryContractRegistry;
pub use council_registry::InMemoryCouncilRegistry;
pub use deliberation_repository::InMemoryDeliberationRepository;
pub use statistics::InMemoryStatistics;
