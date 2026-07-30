//! [`CeremonyDefinitionPublicationPort`] — immutable storage for
//! published definitions.
//!
//! Deliberately separate from
//! [`CeremonyDefinitionRepositoryPort`](super::CeremonyDefinitionRepositoryPort),
//! whose `save` overwrites. An instance started from an ad-hoc
//! definition and one bound to a published version are not the same
//! act, and an audit that cannot tell them apart cannot support the
//! second. Keeping the two paths separate is what keeps the difference
//! visible.

use async_trait::async_trait;

use crate::entities::{PublicationOutcome, PublishedCeremonyDefinition};
use crate::error::DomainError;
use crate::value_objects::{CeremonyName, CeremonyVersion};

#[async_trait]
pub trait CeremonyDefinitionPublicationPort: Send + Sync {
    /// Publish, or report why the version is unavailable.
    ///
    /// Reading the current occupant and writing must be one
    /// indivisible step: checked beforehand, the answer is already
    /// stale by the time the write lands, and two callers could publish
    /// different content under one version.
    async fn publish(
        &self,
        definition: PublishedCeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError>;

    async fn published(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<Option<PublishedCeremonyDefinition>, DomainError>;

    /// The catalogue. Bounded by design rather than by a limit: a
    /// published catalogue that needs pagination has a curation problem
    /// before it has a query problem.
    async fn catalogue(&self) -> Result<Vec<PublishedCeremonyDefinition>, DomainError>;
}
