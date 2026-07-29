//! [`CeremonyTranscriptStorePort`] — pluggable storage for the
//! transcript a ceremony accumulates.
//!
//! The port does two things: append a contribution, and read back what
//! has accumulated. That is a transcript, and naming it context invited
//! a store of everything a ceremony might want to know — memory,
//! evidence, retrieved facts — behind an interface that can only append
//! and replay.
//!
//! Those are separate concerns with separate lifetimes and separate
//! trust levels, and they get their own ports. This one is deliberately
//! narrow.
//!
//! Choreographer stays agnostic about where the transcript lives. The
//! default deployment keeps it in memory; a host can back it with
//! anything by implementing this port.

use async_trait::async_trait;

use crate::error::DomainError;
use crate::value_objects::{CeremonyId, CeremonyStepContribution, CeremonyTranscript};

/// Stores and retrieves the transcript a ceremony accumulates as its
/// steps execute.
#[async_trait]
pub trait CeremonyTranscriptStorePort: Send + Sync {
    /// Append a step's `contribution` to the transcript of `instance_id`.
    async fn append(
        &self,
        instance_id: &CeremonyId,
        contribution: CeremonyStepContribution,
    ) -> Result<(), DomainError>;

    /// The transcript accumulated for `instance_id` so far; an empty
    /// transcript when the ceremony has produced nothing yet.
    async fn transcript(&self, instance_id: &CeremonyId)
        -> Result<CeremonyTranscript, DomainError>;
}
