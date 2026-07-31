use serde::{Deserialize, Serialize};

/// How sure the one who said it was.
///
/// Three degrees and no fourth for "not sure enough to say". A
/// relation is an assertion about why something happened, and an
/// assertion nobody will stand behind is not a weak reason — it is
/// not a reason. A caller who would have chosen "unknown" has the
/// option of not making the claim.
///
/// This is what keeps a later session from reading every explanation
/// as equally settled, which is the failure that makes a memory worse
/// than no memory: a guess and a certainty, side by side, in the same
/// typeface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryConfidence {
    /// Verified, or seen directly.
    High,
    /// Reasoned from what was known, and worth acting on.
    Medium,
    /// Plausible, offered so a later session knows it was considered.
    Low,
}

impl MemoryConfidence {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}
