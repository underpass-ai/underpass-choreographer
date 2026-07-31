use serde::{Deserialize, Serialize};

/// What a remembered thing is.
///
/// Four kinds, and no fifth for "everything else". Memory worth
/// navigating is made of decisions and what led to them; a transcript
/// is the raw material, not the memory, and leaving it no kind of its
/// own is how that stays true without anyone having to police it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEntryKind {
    /// Something was settled, by a person or by the engine.
    Decision,
    /// Something was seen to be the case.
    Observation,
    /// Something was ruled out, or required.
    Constraint,
    /// What came of the work.
    Outcome,
}

impl MemoryEntryKind {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Observation => "observation",
            Self::Constraint => "constraint",
            Self::Outcome => "outcome",
        }
    }
}
