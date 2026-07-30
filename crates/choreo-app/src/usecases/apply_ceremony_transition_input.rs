use choreo_core::value_objects::{CeremonyId, RoleId, TransitionTrigger};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCeremonyTransitionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) role_id: RoleId,
    pub(crate) trigger: TransitionTrigger,
}

impl ApplyCeremonyTransitionInput {
    #[must_use]
    pub fn new(instance_id: CeremonyId, role_id: RoleId, trigger: TransitionTrigger) -> Self {
        Self {
            instance_id,
            role_id,
            trigger,
        }
    }

    #[must_use]
    pub fn instance_id(&self) -> &CeremonyId {
        &self.instance_id
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
