use choreo_core::value_objects::{
    CeremonyId, CeremonyName, CeremonyVersion, RoleId, TransitionTrigger,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCeremonyTransitionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) definition_name: CeremonyName,
    pub(crate) definition_version: CeremonyVersion,
    pub(crate) role_id: RoleId,
    pub(crate) trigger: TransitionTrigger,
}

impl ApplyCeremonyTransitionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        definition_name: CeremonyName,
        definition_version: CeremonyVersion,
        role_id: RoleId,
        trigger: TransitionTrigger,
    ) -> Self {
        Self {
            instance_id,
            definition_name,
            definition_version,
            role_id,
            trigger,
        }
    }

    #[must_use]
    pub fn instance_id(&self) -> &CeremonyId {
        &self.instance_id
    }

    #[must_use]
    pub fn definition_name(&self) -> &CeremonyName {
        &self.definition_name
    }

    #[must_use]
    pub fn definition_version(&self) -> &CeremonyVersion {
        &self.definition_version
    }

    #[must_use]
    pub fn role_id(&self) -> &RoleId {
        &self.role_id
    }

    #[must_use]
    pub fn trigger(&self) -> &TransitionTrigger {
        &self.trigger
    }
}
