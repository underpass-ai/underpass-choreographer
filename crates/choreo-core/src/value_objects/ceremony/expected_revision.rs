use serde::{Deserialize, Serialize};

use super::CeremonyRevision;

/// What the caller believes is stored, checked before a commit lands.
///
/// Creating and updating are distinguished on purpose: without
/// [`ExpectedRevision::New`], two callers could both believe they are
/// starting a ceremony and the second would silently overwrite the
/// first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "expected")]
pub enum ExpectedRevision {
    /// Nothing is stored for this ceremony yet.
    New,
    /// Exactly this revision is stored.
    Exactly(CeremonyRevision),
}

impl ExpectedRevision {
    #[must_use]
    pub fn matches(self, stored: Option<CeremonyRevision>) -> bool {
        match (self, stored) {
            (Self::New, None) => true,
            (Self::Exactly(expected), Some(stored)) => expected == stored,
            _ => false,
        }
    }

    /// The revision a commit produces when this expectation holds.
    #[must_use]
    pub fn resulting_revision(self) -> CeremonyRevision {
        match self {
            Self::New => CeremonyRevision::INITIAL,
            Self::Exactly(stored) => stored.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_matches_only_an_absent_ceremony() {
        assert!(ExpectedRevision::New.matches(None));
        assert!(!ExpectedRevision::New.matches(Some(CeremonyRevision::INITIAL)));
    }

    #[test]
    fn exactly_matches_only_that_revision() {
        let expectation = ExpectedRevision::Exactly(CeremonyRevision::INITIAL);

        assert!(expectation.matches(Some(CeremonyRevision::INITIAL)));
        assert!(!expectation.matches(Some(CeremonyRevision::INITIAL.next())));
        assert!(!expectation.matches(None));
    }

    #[test]
    fn a_first_commit_produces_the_initial_revision() {
        assert_eq!(
            ExpectedRevision::New.resulting_revision(),
            CeremonyRevision::INITIAL
        );
        assert_eq!(
            ExpectedRevision::Exactly(CeremonyRevision::INITIAL).resulting_revision(),
            CeremonyRevision::INITIAL.next()
        );
    }
}
