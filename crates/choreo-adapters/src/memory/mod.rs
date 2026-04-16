//! In-memory adapters backing the domain ports that hold state.
//!
//! These are the simplest possible implementations: data lives in a
//! mutex-guarded map inside an [`Arc`]. They are safe to share across
//! tasks and are production-safe for single-replica deployments; for
//! multi-replica use cases, swap them for a persistent adapter.

mod agent_registry;
mod council_registry;
mod deliberation_repository;

pub use agent_registry::InMemoryAgentRegistry;
pub use council_registry::InMemoryCouncilRegistry;
pub use deliberation_repository::InMemoryDeliberationRepository;
