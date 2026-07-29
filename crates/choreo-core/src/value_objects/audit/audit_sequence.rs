use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Position of a record within one ceremony's audit journal.
///
/// Sequences start at 1 and advance by exactly one. A gap is not a
/// missing record to be tolerated: it is evidence that the journal was
/// truncated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditSequence(u64);

impl AuditSequence {
    pub const FIRST: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "audit_sequence",
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }

    /// The sequence that must follow this one.
    ///
    /// Saturates rather than wrapping: a wrapped sequence would let a
    /// journal appear ordered while replaying earlier positions.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    #[must_use]
    pub fn follows(self, previous: Self) -> bool {
        self.0 == previous.0.saturating_add(1)
    }

    #[must_use]
    pub fn is_first(self) -> bool {
        self == Self::FIRST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_rejected() {
        assert!(matches!(
            AuditSequence::new(0),
            Err(DomainError::MustBeNonZero {
                field: "audit_sequence"
            })
        ));
    }

    #[test]
    fn the_first_sequence_is_one() {
        assert_eq!(AuditSequence::FIRST.value(), 1);
        assert!(AuditSequence::FIRST.is_first());
    }

    #[test]
    fn follows_only_accepts_the_immediate_successor() {
        let first = AuditSequence::FIRST;

        assert!(first.next().follows(first));
        assert!(!AuditSequence::new(3).unwrap().follows(first));
        assert!(!first.follows(first));
    }
}
