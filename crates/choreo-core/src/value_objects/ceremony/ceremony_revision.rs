use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// How many times a ceremony instance has been committed.
///
/// The revision lives in the storage contract rather than in the
/// aggregate: it protects a write against a concurrent write, not any
/// invariant of the ceremony itself. Keeping it out of
/// `CeremonyInstance` also keeps the shape a host has already persisted
/// from changing underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CeremonyRevision(u64);

impl CeremonyRevision {
    /// The revision a ceremony reaches on its first successful commit.
    pub const INITIAL: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "ceremony_revision",
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_ceremony_is_at_least_at_the_initial_revision() {
        assert_eq!(CeremonyRevision::INITIAL.value(), 1);
        assert!(CeremonyRevision::new(0).is_err());
    }

    #[test]
    fn revisions_advance_by_one() {
        assert_eq!(CeremonyRevision::INITIAL.next().value(), 2);
    }
}
