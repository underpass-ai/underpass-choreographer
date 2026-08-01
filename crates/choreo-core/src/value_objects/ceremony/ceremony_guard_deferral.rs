//! Recorded deferral of one human ceremony guard.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::value_objects::{AuditActorKind, RoleId};

use super::{CeremonyGuardDeferralContent, GuardName};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyGuardDeferral {
    guard_name: GuardName,
    deferred_by: RoleId,
    deferred_by_kind: AuditActorKind,
    content: CeremonyGuardDeferralContent,
    #[serde(with = "time::serde::rfc3339")]
    deferred_at: OffsetDateTime,
}

impl CeremonyGuardDeferral {
    #[must_use]
    pub fn record(
        guard_name: GuardName,
        deferred_by: RoleId,
        deferred_by_kind: AuditActorKind,
        content: CeremonyGuardDeferralContent,
        deferred_at: OffsetDateTime,
    ) -> Self {
        Self {
            guard_name,
            deferred_by,
            deferred_by_kind,
            content,
            deferred_at,
        }
    }

    #[must_use]
    pub fn guard_name(&self) -> &GuardName {
        &self.guard_name
    }

    /// The seat that deferred. A deferral said what was decided, why,
    /// and what would make it worth revisiting, and left out who —
    /// which is the one of the four nobody can reconstruct later.
    #[must_use]
    pub fn deferred_by(&self) -> &RoleId {
        &self.deferred_by
    }

    /// What kind of party the caller says filled the seat. Declared,
    /// never deduced.
    #[must_use]
    pub fn deferred_by_kind(&self) -> AuditActorKind {
        self.deferred_by_kind
    }

    #[must_use]
    pub fn content(&self) -> &CeremonyGuardDeferralContent {
        &self.content
    }

    #[must_use]
    pub fn deferred_at(&self) -> OffsetDateTime {
        self.deferred_at
    }
}
