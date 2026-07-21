use choreo_core::value_objects::{
    CeremonyGuardDeferralContent, CeremonyId, CeremonyName, CeremonyVersion, GuardName,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferCeremonyGuardInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) definition_name: CeremonyName,
    pub(crate) definition_version: CeremonyVersion,
    pub(crate) guard_name: GuardName,
    pub(crate) content: CeremonyGuardDeferralContent,
}

impl DeferCeremonyGuardInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        definition_name: CeremonyName,
        definition_version: CeremonyVersion,
        guard_name: GuardName,
        content: CeremonyGuardDeferralContent,
    ) -> Self {
        Self {
            instance_id,
            definition_name,
            definition_version,
            guard_name,
            content,
        }
    }
}
