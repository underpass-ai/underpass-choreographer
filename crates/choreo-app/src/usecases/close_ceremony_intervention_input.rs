use choreo_core::value_objects::{AuditActorKind, CeremonyId, CeremonyInterventionId, RoleId};

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // three identifiers; a suffix-free name would say less
pub struct CloseCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    /// What kind of party fills that seat.
    ///
    /// Carried, never worked out. The engine sees a seat and cannot
    /// see what fills it.
    pub(crate) role_kind: AuditActorKind,
}

impl CloseCeremonyInterventionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        role_kind: AuditActorKind,
    ) -> Self {
        Self {
            instance_id,
            intervention_id,
            role_id,
            role_kind,
        }
    }
}
