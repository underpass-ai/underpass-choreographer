use choreo_core::value_objects::{AuditActorKind, CeremonyId, StepId, StepResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteCeremonyStepInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) step_id: StepId,
    pub(crate) result: StepResult,
    /// What kind of party finished it.
    ///
    /// Only the kind. Which seat runs this step is the definition's to
    /// say and the engine reads it there; what filled that seat is
    /// something only the caller can see.
    pub(crate) actor_kind: AuditActorKind,
}

impl CompleteCeremonyStepInput {
    #[must_use]
    pub fn new(
        instance_id: CeremonyId,
        step_id: StepId,
        result: StepResult,
        actor_kind: AuditActorKind,
    ) -> Self {
        Self {
            instance_id,
            step_id,
            result,
            actor_kind,
        }
    }
}
