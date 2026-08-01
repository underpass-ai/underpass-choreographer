use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::value_objects::RoleId;

use super::{StateId, TransitionTrigger};

/// One move a session made, and who made it.
///
/// A session used to keep only the state it was in, which answers
/// where it is and never how it got there. Nothing could be said about
/// a move — not that it happened, not who fired it, and above all not
/// why — because there was nothing to point at.
///
/// The author is optional and that is not a gap. A transition fired by
/// a seat has one; a transition the engine took because its guards
/// came true does not, and naming someone would be inventing them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyTransitionRecord {
    trigger: TransitionTrigger,
    from_state: StateId,
    to_state: StateId,
    #[serde(default)]
    applied_by: Option<RoleId>,
    #[serde(with = "time::serde::rfc3339")]
    applied_at: OffsetDateTime,
}

impl CeremonyTransitionRecord {
    #[must_use]
    pub fn record(
        trigger: TransitionTrigger,
        from_state: StateId,
        to_state: StateId,
        applied_by: Option<RoleId>,
        applied_at: OffsetDateTime,
    ) -> Self {
        Self {
            trigger,
            from_state,
            to_state,
            applied_by,
            applied_at,
        }
    }

    #[must_use]
    pub fn trigger(&self) -> &TransitionTrigger {
        &self.trigger
    }

    #[must_use]
    pub fn from_state(&self) -> &StateId {
        &self.from_state
    }

    #[must_use]
    pub fn to_state(&self) -> &StateId {
        &self.to_state
    }

    /// The seat that fired it, where one did.
    #[must_use]
    pub fn applied_by(&self) -> Option<&RoleId> {
        self.applied_by.as_ref()
    }

    #[must_use]
    pub fn applied_at(&self) -> OffsetDateTime {
        self.applied_at
    }
}
