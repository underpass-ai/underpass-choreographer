//! [`AuditJournalPort`] — append-only storage for a ceremony's audit
//! journal.
//!
//! The engine owns what a record is, how it is sealed and how a chain
//! is verified. The host owns where records live. This port is that
//! boundary, and the conformance suite is what keeps it from being an
//! unverifiable promise.

use async_trait::async_trait;

use crate::entities::{AuditFact, AuditRecord};
use crate::error::DomainError;
use crate::value_objects::CeremonyId;

#[async_trait]
pub trait AuditJournalPort: Send + Sync {
    /// Seal `fact` against the current head of its ceremony's journal
    /// and append it.
    ///
    /// The caller supplies the fact and never its position: reading the
    /// head, sealing and appending must be one indivisible step, or two
    /// concurrent callers would fork the chain at the same sequence.
    /// Implementations that cannot make it indivisible do not implement
    /// this port.
    async fn append(&self, fact: AuditFact) -> Result<AuditRecord, DomainError>;

    /// The last record written for a ceremony, if any.
    async fn head(&self, ceremony_id: &CeremonyId) -> Result<Option<AuditRecord>, DomainError>;

    /// Every record for a ceremony, in the order it was written.
    ///
    /// Order is part of the contract, not a convenience: a verifier
    /// given records out of order cannot distinguish a reordered
    /// journal from a reordering read.
    async fn records(&self, ceremony_id: &CeremonyId) -> Result<Vec<AuditRecord>, DomainError>;
}
