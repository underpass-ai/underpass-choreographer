use choreo_core::value_objects::{
    AuditActorKind, CeremonyGuardDeferralContent, CeremonyId, GuardName, RoleId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferCeremonyGuardInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) guard_name: GuardName,
    pub(crate) content: CeremonyGuardDeferralContent,
    /// The seat deferring. A deferral already records what was
    /// decided, why, and what would revisit it; who is the fourth, and
    /// the only one nobody can reconstruct afterwards.
    pub(crate) role_id: RoleId,
    /// What kind of party the caller says filled that seat.
    /// Declared, not deduced.
    pub(crate) role_kind: AuditActorKind,
}

impl DeferCeremonyGuardInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        guard_name: GuardName,
        content: CeremonyGuardDeferralContent,
        role_id: RoleId,
        role_kind: AuditActorKind,
    ) -> Self {
        Self {
            instance_id,
            guard_name,
            content,
            role_id,
            role_kind,
        }
    }
    #[must_use]
    pub fn guard_name(&self) -> &GuardName {
        &self.guard_name
    }

    #[must_use]
    pub fn content(&self) -> &CeremonyGuardDeferralContent {
        &self.content
    }
}
