use serde::{Deserialize, Serialize};

use super::{StepId, TransitionTrigger};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RoleAction {
    Step(StepId),
    Transition(TransitionTrigger),
    RequestIntervention,
    RespondToIntervention,
}

impl RoleAction {
    #[must_use]
    pub fn step(step_id: StepId) -> Self {
        Self::Step(step_id)
    }

    #[must_use]
    pub fn transition(trigger: TransitionTrigger) -> Self {
        Self::Transition(trigger)
    }

    #[must_use]
    pub const fn request_intervention() -> Self {
        Self::RequestIntervention
    }

    #[must_use]
    pub const fn respond_to_intervention() -> Self {
        Self::RespondToIntervention
    }

    #[must_use]
    pub fn from_capability_label(label: &str) -> Option<Self> {
        match label {
            "request_intervention" => Some(Self::RequestIntervention),
            "respond_to_intervention" => Some(Self::RespondToIntervention),
            _ => None,
        }
    }

    #[must_use]
    pub fn step_id(&self) -> Option<&StepId> {
        match self {
            Self::Step(step_id) => Some(step_id),
            Self::Transition(_) | Self::RequestIntervention | Self::RespondToIntervention => None,
        }
    }

    #[must_use]
    pub fn transition_trigger(&self) -> Option<&TransitionTrigger> {
        match self {
            Self::Step(_) | Self::RequestIntervention | Self::RespondToIntervention => None,
            Self::Transition(trigger) => Some(trigger),
        }
    }
}
