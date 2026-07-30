use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespondToCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    pub(crate) content: CeremonyInterventionContent,
}

impl RespondToCeremonyInterventionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        content: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            intervention_id,
            role_id,
            content,
        }
    }
}
