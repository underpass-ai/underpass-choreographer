use choreo_core::value_objects::{CeremonyId, GuardName, RoleId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveCeremonyGuardInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) guard_name: GuardName,
    /// The seat approving. An approval that cannot name one is a
    /// receipt asserting a human decision nobody can be shown to have
    /// taken.
    pub(crate) role_id: RoleId,
}

impl ApproveCeremonyGuardInput {
    #[must_use]
    pub fn new(instance_id: CeremonyId, guard_name: GuardName, role_id: RoleId) -> Self {
        Self {
            instance_id,
            guard_name,
            role_id,
        }
    }
}
