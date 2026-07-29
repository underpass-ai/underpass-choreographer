use serde::{Deserialize, Serialize};

use super::AuditSequence;

/// How a journal failed verification.
///
/// Each variant names a distinct way a chain can be attacked, because
/// "the audit is broken" is not actionable and "the record at position
/// 7 no longer matches its digest" is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "defect")]
pub enum AuditChainDefect {
    /// The journal does not open at the first position — records were
    /// removed from the front.
    DoesNotStartAtTheBeginning { found: AuditSequence },

    /// A record's content no longer produces its own digest.
    DigestAltered { at: AuditSequence },

    /// The opening record claims a predecessor, or a later record
    /// claims none. Either way a link was rewritten.
    UnexpectedRoot { at: AuditSequence },

    /// A position was skipped or repeated — records were removed from
    /// the middle, or reordered.
    SequenceBroken {
        expected: AuditSequence,
        found: AuditSequence,
    },

    /// A record names a predecessor digest that is not the digest of
    /// the record before it. Something was substituted.
    LinkBroken { at: AuditSequence },

    /// A record belongs to a different ceremony — journals were
    /// grafted together.
    ForeignCeremony { at: AuditSequence },
}

impl AuditChainDefect {
    /// Where the journal stopped being trustworthy.
    #[must_use]
    pub fn at(self) -> AuditSequence {
        match self {
            Self::DoesNotStartAtTheBeginning { found } | Self::SequenceBroken { found, .. } => {
                found
            }
            Self::DigestAltered { at }
            | Self::UnexpectedRoot { at }
            | Self::LinkBroken { at }
            | Self::ForeignCeremony { at } => at,
        }
    }
}
