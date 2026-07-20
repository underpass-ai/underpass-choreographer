use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
    CeremonyInterventionTarget, CeremonyName, CeremonyVersion, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) definition_name: CeremonyName,
    pub(crate) definition_version: CeremonyVersion,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    pub(crate) kind: CeremonyInterventionKind,
    pub(crate) target: CeremonyInterventionTarget,
    pub(crate) content: CeremonyInterventionContent,
}

impl RequestCeremonyInterventionInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        definition_name: CeremonyName,
        definition_version: CeremonyVersion,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        kind: CeremonyInterventionKind,
        target: CeremonyInterventionTarget,
        content: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            definition_name,
            definition_version,
            intervention_id,
            role_id,
            kind,
            target,
            content,
        }
    }
}
