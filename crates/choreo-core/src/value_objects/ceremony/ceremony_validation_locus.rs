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

/// Where a finding points, said in words.
///
/// The serialized form names the element for a machine; this names it
/// for whoever is reading the explanation. They are different jobs:
/// prose that quotes a JSON object at the reader is prose only by
/// accident.
impl std::fmt::Display for CeremonyValidationLocus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definition => write!(f, "the definition"),
            Self::Input { input } => write!(f, "input `{}`", input.as_str()),
            Self::Output { output } => write!(f, "output `{}`", output.as_str()),
            Self::State { state } => write!(f, "state `{}`", state.as_str()),
            Self::Transition { from, trigger } => write!(
                f,
                "transition `{}` out of state `{}`",
                trigger.as_str(),
                from.as_str()
            ),
            Self::Step { step } => write!(f, "step `{}`", step.as_str()),
            Self::Guard { guard } => write!(f, "guard `{}`", guard.as_str()),
            Self::Role { role } => write!(f, "role `{}`", role.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locus_names_the_element_it_points_at() {
        assert_eq!(
            CeremonyValidationLocus::Definition.to_string(),
            "the definition"
        );
        assert_eq!(
            CeremonyValidationLocus::state(StateId::new("REVIEWING").unwrap()).to_string(),
            "state `REVIEWING`"
        );
        assert_eq!(
            CeremonyValidationLocus::transition(
                StateId::new("OPENING").unwrap(),
                TransitionTrigger::new("context_shared").unwrap(),
            )
            .to_string(),
            "transition `context_shared` out of state `OPENING`"
        );
        assert_eq!(
            CeremonyValidationLocus::guard(GuardName::new("human_approved").unwrap()).to_string(),
            "guard `human_approved`"
        );
    }
}
