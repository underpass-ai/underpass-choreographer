use choreo_core::value_objects::{AuditActorKind, CeremonyId, RoleId, TransitionTrigger};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCeremonyTransitionInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) role_id: RoleId,
    /// What kind of party is firing the trigger.
    ///
    /// The seat comes from the definition, which says which seat was
    /// required. What filled it is something only the caller can see,
    /// so it is carried rather than worked out.
    pub(crate) role_kind: AuditActorKind,
    pub(crate) trigger: TransitionTrigger,
}

impl ApplyCeremonyTransitionInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        role_id: RoleId,
        role_kind: AuditActorKind,
        trigger: TransitionTrigger,
    ) -> Self {
        Self {
            instance_id,
            role_id,
            role_kind,
            trigger,
        }
    }

    #[must_use]
    pub fn role_kind(&self) -> AuditActorKind {
        self.role_kind
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
