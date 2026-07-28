use serde::{Deserialize, Serialize};

use super::{GuardName, InputName, OutputName, RoleId, StateId, StepId, TransitionTrigger};

/// The exact element of a ceremony definition a validation finding
/// refers to.
///
/// A typed error says *what* is wrong; the locus says *which* element
/// is wrong. Both are required for an author — human or agent — to
/// correct a draft without guessing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CeremonyValidationLocus {
    /// The definition as a whole, when no narrower element applies.
    Definition,
    Input {
        input: InputName,
    },
    Output {
        output: OutputName,
    },
    State {
        state: StateId,
    },
    Transition {
        from: StateId,
        trigger: TransitionTrigger,
    },
    Step {
        step: StepId,
    },
    Guard {
        guard: GuardName,
    },
    Role {
        role: RoleId,
    },
}

impl CeremonyValidationLocus {
    #[must_use]
    pub fn input(input: InputName) -> Self {
        Self::Input { input }
    }

    #[must_use]
    pub fn output(output: OutputName) -> Self {
        Self::Output { output }
    }

    #[must_use]
    pub fn state(state: StateId) -> Self {
        Self::State { state }
    }

    #[must_use]
    pub fn transition(from: StateId, trigger: TransitionTrigger) -> Self {
        Self::Transition { from, trigger }
    }

    #[must_use]
    pub fn step(step: StepId) -> Self {
        Self::Step { step }
    }

    #[must_use]
    pub fn guard(guard: GuardName) -> Self {
        Self::Guard { guard }
    }

    #[must_use]
    pub fn role(role: RoleId) -> Self {
        Self::Role { role }
    }
}
