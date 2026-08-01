use choreo_core::value_objects::{
    AuditActorKind, CeremonyEvidenceSourceId, CeremonyId, CeremonyInterventionContent,
    CeremonyInterventionId, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectCeremonyEvidenceInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    /// What kind of party fills that seat.
    ///
    /// Carried, never worked out. The engine sees a seat and cannot
    /// see what fills it.
    pub(crate) role_kind: AuditActorKind,
    pub(crate) source_id: CeremonyEvidenceSourceId,
    pub(crate) query: CeremonyInterventionContent,
}

impl CollectCeremonyEvidenceInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        role_kind: AuditActorKind,
        source_id: CeremonyEvidenceSourceId,
        query: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            intervention_id,
            role_id,
            role_kind,
            source_id,
            query,
        }
    }
}
