use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::value_objects::{GuardName, RoleId};

/// A human guard, and who let it through.
///
/// The engine used to record that a guard had been approved and not by
/// whom. That is the one fact a receipt cannot do without: an approval
/// with no approver is indistinguishable from nobody having looked,
/// which is exactly what the deferral beside it already refuses to be.
///
/// It records a seat rather than a person. Who filled it is the host's
/// to know — this engine has roles, and a host that binds people to
/// them can say more. Recording the seat and letting the host name the
/// person is the honest division; inventing a person here would not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyGuardApproval {
    guard_name: GuardName,
    approved_by: RoleId,
    #[serde(with = "time::serde::rfc3339")]
    approved_at: OffsetDateTime,
}

impl CeremonyGuardApproval {
    #[must_use]
    pub fn record(guard_name: GuardName, approved_by: RoleId, approved_at: OffsetDateTime) -> Self {
        Self {
            guard_name,
            approved_by,
            approved_at,
        }
    }

    #[must_use]
    pub fn guard_name(&self) -> &GuardName {
        &self.guard_name
    }

    #[must_use]
    pub fn approved_by(&self) -> &RoleId {
        &self.approved_by
    }

    #[must_use]
    pub fn approved_at(&self) -> OffsetDateTime {
        self.approved_at
    }
}
