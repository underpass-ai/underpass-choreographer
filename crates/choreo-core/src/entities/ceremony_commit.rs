//! [`CeremonyCommit`] — everything one step of a ceremony changes.
//!
//! State, audit and publication are three claims about the same moment.
//! Saving them separately lets a process die between two of them and
//! leave a journal that disagrees with the state, or a message that
//! reports something that was never stored. They travel together so
//! they can land together.

use crate::entities::{AuditFact, AuditRecord, CeremonyInstance};
use crate::error::DomainError;
use crate::value_objects::{CeremonyRevision, ExpectedRevision, OutboxMessage};

/// The unit that is committed, all of it or none of it.
#[derive(Debug, Clone, PartialEq)]
pub struct CeremonyCommit {
    instance: CeremonyInstance,
    expected_revision: ExpectedRevision,
    facts: Vec<AuditFact>,
    messages: Vec<OutboxMessage>,
}

impl CeremonyCommit {
    /// Every fact must belong to the instance being committed.
    ///
    /// Rejected here rather than in the adapter: a commit that mixes
    /// ceremonies has no correct interpretation, and every
    /// implementation would otherwise have to discover that
    /// independently.
    pub fn new(
        instance: CeremonyInstance,
        expected_revision: ExpectedRevision,
        facts: impl IntoIterator<Item = AuditFact>,
        messages: impl IntoIterator<Item = OutboxMessage>,
    ) -> Result<Self, DomainError> {
        let facts = facts.into_iter().collect::<Vec<_>>();
        if facts.iter().any(|fact| &fact.ceremony_id != instance.id()) {
            return Err(DomainError::InvariantViolated {
                reason: "a commit cannot carry audit facts from another ceremony",
            });
        }
        Ok(Self {
            instance,
            expected_revision,
            facts,
            messages: messages.into_iter().collect(),
        })
    }

    #[must_use]
    pub fn instance(&self) -> &CeremonyInstance {
        &self.instance
    }

    #[must_use]
    pub fn expected_revision(&self) -> ExpectedRevision {
        self.expected_revision
    }

    #[must_use]
    pub fn facts(&self) -> &[AuditFact] {
        &self.facts
    }

    #[must_use]
    pub fn messages(&self) -> &[OutboxMessage] {
        &self.messages
    }

    /// Consume the commit into the parts an adapter writes.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        CeremonyInstance,
        ExpectedRevision,
        Vec<AuditFact>,
        Vec<OutboxMessage>,
    ) {
        (
            self.instance,
            self.expected_revision,
            self.facts,
            self.messages,
        )
    }
}

/// What a commit did.
///
/// A revision conflict is an outcome, not an error: it is the expected
/// result of two callers working on the same ceremony, and the caller
/// must reload and decide rather than treat it as a defect. Making it a
/// variant of the return type is what stops it from being ignored.
#[derive(Debug, Clone, PartialEq)]
pub enum CommitOutcome {
    Committed {
        revision: CeremonyRevision,
        records: Vec<AuditRecord>,
    },
    Conflict {
        expected: ExpectedRevision,
        stored: Option<CeremonyRevision>,
    },
}

impl CommitOutcome {
    #[must_use]
    pub fn committed_revision(&self) -> Option<CeremonyRevision> {
        match self {
            Self::Committed { revision, .. } => Some(*revision),
            Self::Conflict { .. } => None,
        }
    }

    #[must_use]
    pub fn records(&self) -> &[AuditRecord] {
        match self {
            Self::Committed { records, .. } => records,
            Self::Conflict { .. } => &[],
        }
    }

    #[must_use]
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Conflict { .. })
    }
}
