use choreo_core::value_objects::{AuditActorKind, CeremonyId, GuardName, RoleId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveCeremonyGuardInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) guard_name: GuardName,
    /// The seat approving. An approval that cannot name one is a
    /// receipt asserting a human decision nobody can be shown to have
    /// taken.
    pub(crate) role_id: RoleId,
    /// What kind of party the caller says filled that seat.
    ///
    /// Declared, not deduced. That the guard demands a human says one
    /// was required, not that one turned up, and a receipt that read
    /// compliance off the requirement would assert the very thing
    /// nobody can demonstrate.
    pub(crate) role_kind: AuditActorKind,
}

impl ApproveCeremonyGuardInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        guard_name: GuardName,
        role_id: RoleId,
        role_kind: AuditActorKind,
    ) -> Self {
        Self {
            instance_id,
            guard_name,
            role_id,
            role_kind,
        }
    }
}
