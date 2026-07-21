use crate::entities::CeremonyEvidencePack;
use crate::error::DomainError;
use crate::value_objects::{
    CeremonyContext, CeremonyEvidenceSourceId, CeremonyId, CeremonyInterventionContent,
    CeremonyInterventionId, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeremonyEvidenceRequest {
    instance_id: CeremonyId,
    intervention_id: CeremonyInterventionId,
    role_id: RoleId,
    source_id: CeremonyEvidenceSourceId,
    query: CeremonyInterventionContent,
    context: CeremonyContext,
}

impl CeremonyEvidenceRequest {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        source_id: CeremonyEvidenceSourceId,
        query: CeremonyInterventionContent,
        context: CeremonyContext,
    ) -> Self {
        Self {
            instance_id,
            intervention_id,
            role_id,
            source_id,
            query,
            context,
        }
    }

    #[must_use]
    pub fn instance_id(&self) -> &CeremonyId {
        &self.instance_id
    }

    #[must_use]
    pub fn intervention_id(&self) -> &CeremonyInterventionId {
        &self.intervention_id
    }

    #[must_use]
    pub fn role_id(&self) -> &RoleId {
        &self.role_id
    }

    #[must_use]
    pub fn source_id(&self) -> &CeremonyEvidenceSourceId {
        &self.source_id
    }

    #[must_use]
    pub fn query(&self) -> &CeremonyInterventionContent {
        &self.query
    }

    #[must_use]
    pub fn context(&self) -> &CeremonyContext {
        &self.context
    }

    pub fn ensure_matches(&self, pack: &CeremonyEvidencePack) -> Result<(), DomainError> {
        if pack.source_id() == &self.source_id {
            Ok(())
        } else {
            Err(DomainError::InvariantViolated {
                reason: "ceremony evidence source returned a pack for another source",
            })
        }
    }
}
