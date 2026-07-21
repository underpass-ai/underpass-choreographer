use choreo_core::value_objects::{
    CeremonyEvidenceSourceId, CeremonyId, CeremonyInterventionContent, CeremonyInterventionId,
    CeremonyName, CeremonyVersion, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectCeremonyEvidenceInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) definition_name: CeremonyName,
    pub(crate) definition_version: CeremonyVersion,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    pub(crate) source_id: CeremonyEvidenceSourceId,
    pub(crate) query: CeremonyInterventionContent,
}

impl CollectCeremonyEvidenceInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        definition_name: CeremonyName,
        definition_version: CeremonyVersion,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        source_id: CeremonyEvidenceSourceId,
        query: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            definition_name,
            definition_version,
            intervention_id,
            role_id,
            source_id,
            query,
        }
    }
}
