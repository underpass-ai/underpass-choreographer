//! [`CeremonyInstanceView`] — what a caller needs to know about a live
//! working session.
//!
//! Derived once, here, and rendered by whoever is asking: the embedded
//! adapter turns it into JSON, the gRPC adapter into protobuf. Each
//! computing its own would make "the same working session over either
//! transport" a claim maintained by hand, and it would hold only until
//! one of them changed.
//!
//! Only the derived facts live in the view. Interventions, guard
//! deferrals and context are carried by the instance already, so the
//! view lends it out rather than copying it — a projection that
//! re-modelled half the domain would be a second domain.

use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    CeremonyStep, CeremonyTransition, GuardCondition, GuardName, StepExecutionRecord, StepId,
};

/// A declared step paired with what has happened to it.
#[derive(Debug, Clone, Copy)]
pub struct CeremonyStepView<'a> {
    step: &'a CeremonyStep,
    record: &'a StepExecutionRecord,
}

impl<'a> CeremonyStepView<'a> {
    #[must_use]
    pub fn step(&self) -> &'a CeremonyStep {
        self.step
    }

    #[must_use]
    pub fn record(&self) -> &'a StepExecutionRecord {
        self.record
    }
}

/// A guard on a transition, and whether it holds right now.
#[derive(Debug, Clone, Copy)]
pub struct CeremonyGuardView<'a> {
    name: &'a GuardName,
    human: bool,
    satisfied: bool,
}

impl<'a> CeremonyGuardView<'a> {
    #[must_use]
    pub fn name(&self) -> &'a GuardName {
        self.name
    }

    /// Whether only a person can satisfy it. This is what separates
    /// "blocked on work" from "blocked on you".
    #[must_use]
    pub fn is_human(&self) -> bool {
        self.human
    }

    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        self.satisfied
    }
}

/// A transition leaving the current state.
#[derive(Debug, Clone)]
pub struct CeremonyTransitionView<'a> {
    transition: &'a CeremonyTransition,
    enabled: bool,
    guards: Vec<CeremonyGuardView<'a>>,
}

impl<'a> CeremonyTransitionView<'a> {
    #[must_use]
    pub fn transition(&self) -> &'a CeremonyTransition {
        self.transition
    }

    /// Whether every guard holds, so this transition can be applied.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn guards(&self) -> &[CeremonyGuardView<'a>] {
        &self.guards
    }

    /// Whether everything a machine can settle is settled, so the only
    /// thing left is a person.
    #[must_use]
    fn waits_only_on_people(&self) -> bool {
        self.guards
            .iter()
            .filter(|guard| !guard.human)
            .all(CeremonyGuardView::is_satisfied)
    }
}

/// The derived state of one working session.
#[derive(Debug, Clone)]
pub struct CeremonyInstanceView<'a> {
    instance: &'a CeremonyInstance,
    definition: &'a CeremonyDefinition,
    steps: Vec<CeremonyStepView<'a>>,
    transitions: Vec<CeremonyTransitionView<'a>>,
    waiting_for_human: Vec<&'a GuardName>,
    next_step_id: Option<&'a StepId>,
    completed: bool,
}

impl<'a> CeremonyInstanceView<'a> {
    /// Derive the view.
    ///
    /// Fails rather than panics when a declared step has no record: a
    /// definition and an instance that do not correspond is a real
    /// inconsistency, and a projection is the wrong place to decide it
    /// cannot happen.
    pub fn project(
        instance: &'a CeremonyInstance,
        definition: &'a CeremonyDefinition,
    ) -> Result<Self, DomainError> {
        let steps = definition
            .steps_in_declaration_order()
            .map(|step| {
                instance
                    .step_record(step.id())
                    .map(|record| CeremonyStepView { step, record })
                    .ok_or(DomainError::NotFound {
                        what: "ceremony_step_record",
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let transitions = definition
            .available_transitions(instance.current_state())
            .map(|transition| {
                let guards = transition
                    .required_guards()
                    .iter()
                    .map(|name| {
                        let guard = definition.guards().get(name).ok_or(DomainError::NotFound {
                            what: "ceremony_transition.guard",
                        })?;
                        Ok(CeremonyGuardView {
                            name,
                            human: matches!(guard.condition(), GuardCondition::HumanApproval),
                            satisfied: guard
                                .is_satisfied(instance.step_records(), instance.context()),
                        })
                    })
                    .collect::<Result<Vec<_>, DomainError>>()?;
                Ok(CeremonyTransitionView {
                    transition,
                    enabled: definition.guards_are_satisfied(
                        transition,
                        instance.step_records(),
                        instance.context(),
                    ),
                    guards,
                })
            })
            .collect::<Result<Vec<_>, DomainError>>()?;

        // Only guards on transitions whose automated conditions already
        // hold. A human guard behind unfinished work is not waiting on
        // anybody yet, and reporting it would send someone to approve
        // something that cannot proceed.
        let waiting_for_human = transitions
            .iter()
            .filter(|transition| transition.waits_only_on_people())
            .flat_map(CeremonyTransitionView::guards)
            .filter(|guard| guard.human && !guard.satisfied)
            .map(CeremonyGuardView::name)
            .collect::<Vec<_>>();

        let next_step_id = definition
            .steps_for_state(instance.current_state())
            .find(|step| {
                instance
                    .step_record(step.id())
                    .is_some_and(|record| !record.status().is_success())
            })
            .map(CeremonyStep::id);

        Ok(Self {
            instance,
            definition,
            steps,
            transitions,
            waiting_for_human,
            next_step_id,
            completed: instance.is_completed(definition),
        })
    }

    #[must_use]
    pub fn instance(&self) -> &'a CeremonyInstance {
        self.instance
    }

    #[must_use]
    pub fn definition(&self) -> &'a CeremonyDefinition {
        self.definition
    }

    #[must_use]
    pub fn steps(&self) -> &[CeremonyStepView<'a>] {
        &self.steps
    }

    #[must_use]
    pub fn transitions(&self) -> &[CeremonyTransitionView<'a>] {
        &self.transitions
    }

    /// Guards a person must decide before this session can move.
    #[must_use]
    pub fn waiting_for_human(&self) -> &[&'a GuardName] {
        &self.waiting_for_human
    }

    #[must_use]
    pub fn next_step_id(&self) -> Option<&'a StepId> {
        self.next_step_id
    }

    #[must_use]
    pub fn is_completed(&self) -> bool {
        self.completed
    }
}
