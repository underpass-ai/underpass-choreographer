use choreo_core::value_objects::{CeremonyId, StepId, StepResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteCeremonyStepInput {
    pub(crate) instance_id: CeremonyId,
    pub(crate) step_id: StepId,
    pub(crate) result: StepResult,
}

impl CompleteCeremonyStepInput {
    #[must_use]
    pub fn new(instance_id: CeremonyId, step_id: StepId, result: StepResult) -> Self {
        Self {
            instance_id,
            step_id,
            result,
        }
    }
}
