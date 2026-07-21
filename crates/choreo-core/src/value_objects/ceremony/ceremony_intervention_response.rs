use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::entities::CeremonyEvidencePack;
use crate::error::DomainError;

use super::{CeremonyInterventionContent, RoleId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyInterventionResponse {
    role_id: RoleId,
    content: CeremonyInterventionContent,
    #[serde(default)]
    evidence_pack: Option<CeremonyEvidencePack>,
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
            evidence_pack: None,
            responded_at,
        }
    }

    pub fn from_evidence(
        role_id: RoleId,
        evidence_pack: CeremonyEvidencePack,
        responded_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let content = evidence_pack.intervention_content()?;
        Ok(Self {
            role_id,
            content,
            evidence_pack: Some(evidence_pack),
            responded_at,
        })
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
    pub fn evidence_pack(&self) -> Option<&CeremonyEvidencePack> {
        self.evidence_pack.as_ref()
    }

    #[must_use]
    pub fn responded_at(&self) -> OffsetDateTime {
        self.responded_at
    }
}
