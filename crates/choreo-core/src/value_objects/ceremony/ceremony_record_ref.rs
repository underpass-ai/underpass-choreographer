use serde::{Deserialize, Serialize};

use super::{CeremonyInterventionId, GuardName, StepId};

/// Something a session produced that another thing can point at.
///
/// Typed rather than a string, because the aggregate can then check
/// that both ends of a reason exist. Memory cannot — an edge there may
/// legitimately reach an entry written an hour ago — but a session
/// knows everything it has done, and declining to use that would be
/// letting a reason cite something that never happened.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CeremonyRecordRef {
    /// A step this session ran.
    Step { step_id: StepId },
    /// An agenda item somebody opened.
    AgendaItem { agenda_item: CeremonyInterventionId },
    /// One contribution to an agenda item, by its place in the order
    /// they were made.
    Contribution {
        agenda_item: CeremonyInterventionId,
        ordinal: u32,
    },
    /// A human decision on a guard — an approval or a deferral.
    GuardDecision { guard_name: GuardName },
    /// The nth move this session made, counting from one.
    ///
    /// By position rather than by trigger: a session may fire the same
    /// trigger more than once, and "the move that ended it" has to
    /// name one of them.
    Transition { ordinal: u32 },
}

impl CeremonyRecordRef {
    #[must_use]
    pub fn step(step_id: StepId) -> Self {
        Self::Step { step_id }
    }

    #[must_use]
    pub fn agenda_item(agenda_item: CeremonyInterventionId) -> Self {
        Self::AgendaItem { agenda_item }
    }

    #[must_use]
    pub fn contribution(agenda_item: CeremonyInterventionId, ordinal: u32) -> Self {
        Self::Contribution {
            agenda_item,
            ordinal,
        }
    }

    #[must_use]
    pub fn guard_decision(guard_name: GuardName) -> Self {
        Self::GuardDecision { guard_name }
    }

    #[must_use]
    pub const fn transition(ordinal: u32) -> Self {
        Self::Transition { ordinal }
    }
}
