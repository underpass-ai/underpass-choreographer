use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::value_objects::{AuditActorKind, GuardName, RoleId};

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
///
/// What **kind** of party filled it is declared by whoever made the
/// call, and is not inferred here. A guard requiring human approval
/// says a human was required, not that one turned up, and reading
/// compliance off the requirement is the exact shape of receipt this
/// engine refuses to write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyGuardApproval {
    guard_name: GuardName,
    approved_by: RoleId,
    approved_by_kind: AuditActorKind,
    #[serde(with = "time::serde::rfc3339")]
    approved_at: OffsetDateTime,
}

impl CeremonyGuardApproval {
    #[must_use]
    pub fn record(
        guard_name: GuardName,
        approved_by: RoleId,
        approved_by_kind: AuditActorKind,
        approved_at: OffsetDateTime,
    ) -> Self {
        Self {
            guard_name,
            approved_by,
            approved_by_kind,
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

    /// What kind of party the caller says filled the seat.
    ///
    /// Declared, never deduced. An approval recorded as human because
    /// the guard asked for one would assert the very thing nobody can
    /// demonstrate.
    #[must_use]
    pub fn approved_by_kind(&self) -> AuditActorKind {
        self.approved_by_kind
    }

    #[must_use]
    pub fn approved_at(&self) -> OffsetDateTime {
        self.approved_at
    }
}
