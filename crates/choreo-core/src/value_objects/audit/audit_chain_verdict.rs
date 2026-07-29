use serde::{Deserialize, Serialize};

use super::AuditChainDefect;

/// The outcome of verifying a ceremony's journal.
///
/// Unlike a definition analysis, this does not accumulate. Once a chain
/// breaks, every record after the break is suspect — reporting further
/// defects would suggest the verifier still knows what it is looking
/// at. The first defect is where trust ends, and that is the whole
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "verdict")]
pub enum AuditChainVerdict {
    /// Every record is sealed, positioned and linked as written.
    Intact,
    Broken(AuditChainDefect),
}

impl AuditChainVerdict {
    #[must_use]
    pub fn is_intact(self) -> bool {
        matches!(self, Self::Intact)
    }

    #[must_use]
    pub fn defect(self) -> Option<AuditChainDefect> {
        match self {
            Self::Intact => None,
            Self::Broken(defect) => Some(defect),
        }
    }
}
