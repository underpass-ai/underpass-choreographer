use choreo_core::value_objects::{
    CeremonyId, DurationMs, IdempotencyKey, LeaseOwnerId, RoleId, StepId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartCeremonyStepInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) role_id: RoleId,
    pub(crate) step_id: StepId,
    pub(crate) lease_owner_id: LeaseOwnerId,
    pub(crate) idempotency_key: IdempotencyKey,
    pub(crate) lease_ttl: DurationMs,
}

impl StartCeremonyStepInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        role_id: RoleId,
        step_id: StepId,
        lease_owner_id: LeaseOwnerId,
        idempotency_key: IdempotencyKey,
        lease_ttl: DurationMs,
    ) -> Self {
        Self {
            instance_id,
            role_id,
            step_id,
            lease_owner_id,
            idempotency_key,
            lease_ttl,
        }
    }
}
