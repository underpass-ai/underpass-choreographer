use serde::{Deserialize, Serialize};

use crate::error::DomainError;

use super::{MemoryConfidence, MemoryEntryId, MemoryRelationKind};

/// A reason is short. The long version belongs on the entry it points
/// at, where a reader who wants it can go and get it; an edge carrying
/// paragraphs makes the shape of the reasoning unreadable, which is
/// the one thing an edge is for.
const MAX_WHY: usize = 512;

/// Why one remembered thing led to another.
///
/// This is not an entry with a link attached. **The relation is the
/// reason**: an entry says what was decided or seen, and only the
/// edges say how one thing came from another. A memory of nothing but
/// entries can be read but not followed, and following is how a later
/// session works out how and why something was done.
///
/// So `why` is not optional. A relation without one asserts that two
/// things are connected while declining to say how, which is the shape
/// of a guess written down as a fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRelation {
    from: MemoryEntryId,
    to: MemoryEntryId,
    kind: MemoryRelationKind,
    why: String,
    confidence: MemoryConfidence,
}

impl MemoryRelation {
    /// Relate `from` to `to`, saying how and how sure.
    ///
    /// The endpoints need not both be written by the same call. An
    /// ending explains a decision taken an hour earlier, and a session
    /// that could only relate what it wrote in one breath could never
    /// record that. What it may not do is relate an entry to itself:
    /// nothing explains itself, and an edge that says so is a cycle a
    /// reader will follow forever.
    pub fn new(
        from: MemoryEntryId,
        to: MemoryEntryId,
        kind: MemoryRelationKind,
        why: impl Into<String>,
        confidence: MemoryConfidence,
    ) -> Result<Self, DomainError> {
        if from == to {
            return Err(DomainError::InvariantViolated {
                reason: "a memory entry cannot explain itself",
            });
        }
        let why = why.into();
        let trimmed = why.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_relation.why",
            });
        }
        if trimmed.chars().count() > MAX_WHY {
            return Err(DomainError::FieldTooLong {
                field: "memory_relation.why",
                max: MAX_WHY,
                actual: trimmed.chars().count(),
            });
        }
        Ok(Self {
            from,
            to,
            kind,
            why: trimmed.to_owned(),
            confidence,
        })
    }

    #[must_use]
    pub fn from(&self) -> &MemoryEntryId {
        &self.from
    }

    #[must_use]
    pub fn to(&self) -> &MemoryEntryId {
        &self.to
    }

    #[must_use]
    pub const fn kind(&self) -> MemoryRelationKind {
        self.kind
    }

    /// The reason, in one line.
    #[must_use]
    pub fn why(&self) -> &str {
        &self.why
    }

    #[must_use]
    pub const fn confidence(&self) -> MemoryConfidence {
        self.confidence
    }
}
