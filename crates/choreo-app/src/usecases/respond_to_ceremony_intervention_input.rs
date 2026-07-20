use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, CeremonyName, CeremonyVersion,
    RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespondToCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) definition_name: CeremonyName,
    pub(crate) definition_version: CeremonyVersion,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    pub(crate) content: CeremonyInterventionContent,
}

impl RespondToCeremonyInterventionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        definition_name: CeremonyName,
        definition_version: CeremonyVersion,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        content: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            definition_name,
            definition_version,
            intervention_id,
            role_id,
            content,
        }
    }
}
