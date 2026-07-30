use choreo_core::value_objects::{CeremonyGuardDeferralContent, CeremonyId, GuardName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferCeremonyGuardInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) guard_name: GuardName,
    pub(crate) content: CeremonyGuardDeferralContent,
}

impl DeferCeremonyGuardInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        guard_name: GuardName,
        content: CeremonyGuardDeferralContent,
    ) -> Self {
        Self {
            instance_id,
            guard_name,
            content,
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
