//! Contract suites the engine ships so hosts can prove their adapters.
//!
//! The engine cedes storage, not the contract. A suite here is the
//! difference between a port a host implements and a port a host can be
//! shown to have implemented.
//!
//! Enabled by the `conformance` feature so the suites never enter a
//! production build, while staying available to any host, inside this
//! repository or outside it.

mod audit_journal_conformance;
mod ceremony_definition_publication_conformance;
mod ceremony_unit_of_work_conformance;
mod conformance_fixtures;
mod memory_conformance;
mod outbox_conformance;

pub use audit_journal_conformance::{AuditJournalConformance, ConformanceFailure};
pub use ceremony_definition_publication_conformance::CeremonyDefinitionPublicationConformance;
pub use ceremony_unit_of_work_conformance::CeremonyUnitOfWorkConformance;
pub use memory_conformance::{MemoryConformance, MemoryConformanceFailure};
pub use outbox_conformance::OutboxConformance;
