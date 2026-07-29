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

pub use audit_journal_conformance::{AuditJournalConformance, ConformanceFailure};
