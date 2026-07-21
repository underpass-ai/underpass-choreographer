//! Recorded deferral of one human ceremony guard.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{CeremonyGuardDeferralContent, GuardName};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyGuardDeferral {
    guard_name: GuardName,
    content: CeremonyGuardDeferralContent,
    #[serde(with = "time::serde::rfc3339")]
    deferred_at: OffsetDateTime,
}

impl CeremonyGuardDeferral {
    #[must_use]
    pub fn record(
        guard_name: GuardName,
        content: CeremonyGuardDeferralContent,
        deferred_at: OffsetDateTime,
    ) -> Self {
        Self {
            guard_name,
            content,
            deferred_at,
        }
    }

    #[must_use]
    pub fn guard_name(&self) -> &GuardName {
        &self.guard_name
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
