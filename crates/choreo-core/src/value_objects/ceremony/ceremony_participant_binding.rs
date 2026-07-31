use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::value_objects::Specialty;

use super::RoleId;

/// Who sits in a role's seat for one working session.
///
/// A definition says what a role *does*; it does not say who plays it.
/// Left alone, that is settled by the step's own configuration, the
/// same way for every session the ceremony ever runs. A binding is how
/// one session says otherwise: this review is being done by the panel
/// that knows this system, not by whoever the document names in
/// general.
///
/// What is bound is a specialty, because a specialty is what the
/// engine already resolves a council from. Seating a different panel
/// *is* choosing a different council, so binding a specialty says
/// exactly that and nothing is invented alongside the machinery that
/// already decides who deliberates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyParticipantBinding {
    role_id: RoleId,
    specialty: Specialty,
    #[serde(with = "time::serde::rfc3339")]
    bound_at: OffsetDateTime,
}

impl CeremonyParticipantBinding {
    #[must_use]
    pub fn record(role_id: RoleId, specialty: Specialty, bound_at: OffsetDateTime) -> Self {
        Self {
            role_id,
            specialty,
            bound_at,
        }
    }

    #[must_use]
    pub fn role_id(&self) -> &RoleId {
        &self.role_id
    }

    #[must_use]
    pub fn specialty(&self) -> &Specialty {
        &self.specialty
    }

    #[must_use]
    pub fn bound_at(&self) -> OffsetDateTime {
        self.bound_at
    }
}
