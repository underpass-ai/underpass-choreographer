use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{CeremonyInterventionContent, RoleId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyInterventionResponse {
    role_id: RoleId,
    content: CeremonyInterventionContent,
    #[serde(with = "time::serde::rfc3339")]
    responded_at: OffsetDateTime,
}

impl CeremonyInterventionResponse {
    #[must_use]
    pub fn new(
        role_id: RoleId,
        content: CeremonyInterventionContent,
        responded_at: OffsetDateTime,
    ) -> Self {
        Self {
            role_id,
            content,
            responded_at,
        }
    }

    #[must_use]
    pub fn role_id(&self) -> &RoleId {
        &self.role_id
    }

    #[must_use]
    pub fn content(&self) -> &CeremonyInterventionContent {
        &self.content
    }

    #[must_use]
    pub fn responded_at(&self) -> OffsetDateTime {
        self.responded_at
    }
}
