//! [`CeremonyUnitOfWorkPort`] — the transactional boundary of the
//! engine.
//!
//! The engine owns what must land together; the host owns how. This is
//! the only place where that promise can be kept, so it is stated as a
//! contract and checked by a conformance suite rather than assumed.

use async_trait::async_trait;

use crate::entities::{CeremonyCommit, CommitOutcome};
use crate::error::DomainError;
use crate::value_objects::{CeremonyId, CeremonyRevision};

#[async_trait]
pub trait CeremonyUnitOfWorkPort: Send + Sync {
    /// Store the instance, seal and append its audit facts, and enqueue
    /// its messages — all of it or none of it.
    ///
    /// The expected revision is checked inside the same transaction: a
    /// check performed before it would already be stale by the time the
    /// write happens.
    ///
    /// An error means nothing landed. A [`CommitOutcome::Conflict`]
    /// also means nothing landed, but it is not an error: another
    /// caller got there first, and this one must reload and decide.
    async fn commit(&self, commit: CeremonyCommit) -> Result<CommitOutcome, DomainError>;

    /// The revision currently stored for a ceremony, if any.
    async fn revision(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Result<Option<CeremonyRevision>, DomainError>;
}
