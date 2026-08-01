use choreo_core::value_objects::{
    AuditActorKind, CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespondToCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    /// What kind of party fills that seat.
    ///
    /// Carried, never worked out. The engine sees a seat and cannot
    /// see what fills it.
    pub(crate) role_kind: AuditActorKind,
    pub(crate) content: CeremonyInterventionContent,
}

impl RespondToCeremonyInterventionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        role_kind: AuditActorKind,
        content: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            intervention_id,
            role_id,
            role_kind,
            content,
        }
    }
}
