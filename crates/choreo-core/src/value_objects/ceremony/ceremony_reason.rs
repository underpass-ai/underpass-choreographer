use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::{MemoryConfidence, RoleId};

use super::{CeremonyReasonKind, CeremonyRecordRef};

/// A reason is short. Whatever needs paragraphs belongs on the record
/// it points at, where a reader who wants it can go; an edge carrying
/// an essay makes the shape of the reasoning unreadable, which is the
/// one thing an edge is for.
const MAX_WHY: usize = 512;

/// Why one thing a session produced led to another.
///
/// Not an annotation on a record: **the reason is the edge**. A record
/// says what was decided, seen or done, and only the edges say how one
/// came from another — so a session of records alone can be read and
/// not followed, and following is how anyone later works out how and
/// why something was done.
///
/// Immutable, and not corrected. Changing your mind is asserting
/// another reason that supersedes this one, which keeps what was
/// believed at the time — the thing a later reader asking "what did we
/// think then" cannot do without.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyReason {
    from: CeremonyRecordRef,
    to: CeremonyRecordRef,
    kind: CeremonyReasonKind,
    why: String,
    confidence: MemoryConfidence,
    asserted_by: Option<RoleId>,
    #[serde(with = "time::serde::rfc3339")]
    asserted_at: OffsetDateTime,
}

impl CeremonyReason {
    /// State that `from` came about from `to`, and why.
    ///
    /// `asserted_by` is absent only for what the engine itself
    /// observed. Every other kind requires a seat, and which kinds
    /// those are is not this constructor's to police — the session
    /// holds the definition and the records, and checks it there.
    pub fn new(
        from: CeremonyRecordRef,
        to: CeremonyRecordRef,
        kind: CeremonyReasonKind,
        why: impl Into<String>,
        confidence: MemoryConfidence,
        asserted_by: Option<RoleId>,
        asserted_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        if from == to {
            return Err(DomainError::InvariantViolated {
                reason: "nothing in a session explains itself",
            });
        }
        let why = why.into();
        let trimmed = why.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "ceremony_reason.why",
            });
        }
        if trimmed.chars().count() > MAX_WHY {
            return Err(DomainError::FieldTooLong {
                field: "ceremony_reason.why",
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
            asserted_by,
            asserted_at,
        })
    }

    #[must_use]
    pub fn from(&self) -> &CeremonyRecordRef {
        &self.from
    }

    #[must_use]
    pub fn to(&self) -> &CeremonyRecordRef {
        &self.to
    }

    #[must_use]
    pub const fn kind(&self) -> CeremonyReasonKind {
        self.kind
    }

    /// The reason itself, in one line.
    #[must_use]
    pub fn why(&self) -> &str {
        &self.why
    }

    #[must_use]
    pub const fn confidence(&self) -> MemoryConfidence {
        self.confidence
    }

    /// The seat that said so, where a seat did.
    #[must_use]
    pub fn asserted_by(&self) -> Option<&RoleId> {
        self.asserted_by.as_ref()
    }

    #[must_use]
    pub fn asserted_at(&self) -> OffsetDateTime {
        self.asserted_at
    }
}
