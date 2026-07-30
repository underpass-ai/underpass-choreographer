use choreo_core::value_objects::{
    CeremonyId, CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
    CeremonyInterventionProvenance, CeremonyInterventionTarget, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCeremonyInterventionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) intervention_id: CeremonyInterventionId,
    pub(crate) role_id: RoleId,
    pub(crate) kind: CeremonyInterventionKind,
    pub(crate) target: CeremonyInterventionTarget,
    pub(crate) content: CeremonyInterventionContent,
    pub(crate) provenance: Option<CeremonyInterventionProvenance>,
}

impl RequestCeremonyInterventionInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        kind: CeremonyInterventionKind,
        target: CeremonyInterventionTarget,
        content: CeremonyInterventionContent,
    ) -> Self {
        Self {
            instance_id,
            intervention_id,
            role_id,
            kind,
            target,
            content,
            provenance: None,
        }
    }

    #[must_use]
    pub fn with_provenance(mut self, provenance: CeremonyInterventionProvenance) -> Self {
        self.provenance = Some(provenance);
        self
    }
    #[must_use]
    pub fn intervention_id(&self) -> &CeremonyInterventionId {
        &self.intervention_id
    }

    #[must_use]
    pub fn target(&self) -> &CeremonyInterventionTarget {
        &self.target
    }

    #[must_use]
    pub const fn kind(&self) -> CeremonyInterventionKind {
        self.kind
    }
}
