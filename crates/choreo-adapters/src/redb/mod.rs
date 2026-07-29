//! Embedded persistence on redb.
//!
//! The store the embedded distribution uses: pure Rust, no non-optional
//! dependencies and no C toolchain, so an embedded host stays free of
//! system dependencies. One write transaction spans several tables,
//! which is what the ceremony unit of work needs.
//!
//! redb takes an exclusive lock on its file and serves a single
//! process. That is right for an embedded host and wrong for replicas.

mod error;
mod keys;

pub use ceremony_store::RedbCeremonyStore;

mod ceremony_store;
