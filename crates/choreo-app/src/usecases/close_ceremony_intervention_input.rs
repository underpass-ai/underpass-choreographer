use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionId, CeremonyName, CeremonyVersion, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) definition_name: CeremonyName,
    pub(crate) definition_version: CeremonyVersion,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
}

impl CloseCeremonyInterventionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        definition_name: CeremonyName,
        definition_version: CeremonyVersion,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
    ) -> Self {
        Self {
            instance_id,
            definition_name,
            definition_version,
            intervention_id,
            role_id,
        }
    }
}
