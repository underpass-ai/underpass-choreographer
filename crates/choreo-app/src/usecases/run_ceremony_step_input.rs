use choreo_core::value_objects::{
    AuditActorKind, CeremonyId, DurationMs, IdempotencyKey, LeaseOwnerId, RoleId, StepId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCeremonyStepInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) role_id: RoleId,
    /// What kind of party is running it.
    ///
    /// Carried, never worked out. The engine sees a seat and cannot
    /// see what fills it.
    pub(crate) role_kind: AuditActorKind,
    pub(crate) step_id: StepId,
    pub(crate) lease_owner_id: LeaseOwnerId,
    pub(crate) idempotency_key: IdempotencyKey,
    pub(crate) lease_ttl: DurationMs,
}

impl RunCeremonyStepInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        role_id: RoleId,
        role_kind: AuditActorKind,
        step_id: StepId,
        lease_owner_id: LeaseOwnerId,
        idempotency_key: IdempotencyKey,
        lease_ttl: DurationMs,
    ) -> Self {
        Self {
            instance_id,
            role_id,
            role_kind,
            step_id,
            lease_owner_id,
            idempotency_key,
            lease_ttl,
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
    pub fn step_id(&self) -> &StepId {
        &self.step_id
    }

    #[must_use]
    pub fn lease_owner_id(&self) -> &LeaseOwnerId {
        &self.lease_owner_id
    }

    #[must_use]
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    #[must_use]
    pub const fn lease_ttl(&self) -> DurationMs {
        self.lease_ttl
    }
}
