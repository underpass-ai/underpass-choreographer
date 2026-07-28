//! Shared structural analysis of a ceremony state machine.
//!
//! The same checks serve a published [`CeremonyDefinition`] and a
//! [`CeremonyDefinitionDraft`] that may not be publishable at all, so
//! they operate on borrowed parts instead of on either aggregate.
//!
//! [`CeremonyDefinition`]: super::CeremonyDefinition
//! [`CeremonyDefinitionDraft`]: super::CeremonyDefinitionDraft

use std::collections::{BTreeMap, BTreeSet};

use crate::error::DomainError;
use crate::value_objects::{
    CeremonyGuard, CeremonyRole, CeremonyState, CeremonyStep, CeremonyTransition,
    CeremonyValidationFinding, CeremonyValidationLocus, GuardName, RoleId, StateId, StepId,
};

/// The assembled parts of a ceremony state machine, borrowed for
/// analysis.
pub(super) struct CeremonyDefinitionParts<'a> {
    pub(super) states: &'a BTreeMap<StateId, CeremonyState>,
    pub(super) transitions: &'a [CeremonyTransition],
    pub(super) steps: &'a BTreeMap<StepId, CeremonyStep>,
    pub(super) guards: &'a BTreeMap<GuardName, CeremonyGuard>,
    pub(super) roles: &'a BTreeMap<RoleId, CeremonyRole>,
}

impl CeremonyDefinitionParts<'_> {
    /// Append every structural finding, in check order.
    ///
    /// Order matters: the first blocking finding must be the error
    /// fail-fast construction raises.
    pub(super) fn collect_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        self.collect_initial_state_findings(findings);
        self.collect_transition_graph_findings(findings);
        self.collect_step_findings(findings);
        self.collect_guard_findings(findings);
        self.collect_role_findings(findings);
        self.collect_reachability_findings(findings);
    }

    fn collect_initial_state_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        if self.states.is_empty() {
            findings.push(CeremonyValidationFinding::error(
                CeremonyValidationLocus::Definition,
                DomainError::EmptyCollection {
                    field: "ceremony_definition.states",
                },
            ));
            return;
        }

        let initial_count = self
            .states
            .values()
            .filter(|state| state.is_initial())
            .count();
        if initial_count != 1 {
            findings.push(CeremonyValidationFinding::error(
                CeremonyValidationLocus::Definition,
                DomainError::InvariantViolated {
                    reason: "ceremony definition must have exactly one initial state",
                },
            ));
        }
    }

    fn collect_transition_graph_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        let mut state_trigger_pairs = BTreeSet::new();
        for transition in self.transitions {
            let locus = CeremonyValidationLocus::transition(
                transition.from().clone(),
                transition.trigger().clone(),
            );
            let Some(from) = self.states.get(transition.from()) else {
                findings.push(CeremonyValidationFinding::error(
                    locus,
                    DomainError::NotFound {
                        what: "ceremony_transition.from_state",
                    },
                ));
                continue;
            };
            if !self.states.contains_key(transition.to()) {
                findings.push(CeremonyValidationFinding::error(
                    locus,
                    DomainError::NotFound {
                        what: "ceremony_transition.to_state",
                    },
                ));
                continue;
            }
            if from.is_terminal() {
                findings.push(CeremonyValidationFinding::error(
                    locus,
                    DomainError::InvariantViolated {
                        reason: "terminal ceremony states cannot have outgoing transitions",
                    },
                ));
                continue;
            }
            if !state_trigger_pairs
                .insert((transition.from().clone(), transition.trigger().clone()))
            {
                findings.push(CeremonyValidationFinding::error(
                    locus,
                    DomainError::AlreadyExists {
                        what: "ceremony_transition.state_trigger",
                    },
                ));
                continue;
            }
            for guard_name in transition.required_guards() {
                if !self.guards.contains_key(guard_name) {
                    findings.push(CeremonyValidationFinding::error(
                        locus.clone(),
                        DomainError::NotFound {
                            what: "ceremony_transition.guard",
                        },
                    ));
                }
            }
        }
    }

    fn collect_step_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        for (step_id, step) in self.steps {
            let locus = CeremonyValidationLocus::step(step_id.clone());
            let Some(state) = self.states.get(step.state_id()) else {
                findings.push(CeremonyValidationFinding::error(
                    locus,
                    DomainError::NotFound {
                        what: "ceremony_step.state",
                    },
                ));
                continue;
            };
            if state.is_terminal() {
                findings.push(CeremonyValidationFinding::error(
                    locus,
                    DomainError::InvariantViolated {
                        reason: "terminal ceremony states cannot own executable steps",
                    },
                ));
            }
        }
    }

    fn collect_guard_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        for (guard_name, guard) in self.guards {
            if let Some(step_id) = guard.condition().referenced_step_id() {
                if !self.steps.contains_key(step_id) {
                    findings.push(CeremonyValidationFinding::error(
                        CeremonyValidationLocus::guard(guard_name.clone()),
                        DomainError::NotFound {
                            what: "ceremony_guard.step",
                        },
                    ));
                }
            }
        }
    }

    fn collect_role_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        let transition_triggers = self
            .transitions
            .iter()
            .map(|transition| transition.trigger().clone())
            .collect::<BTreeSet<_>>();

        for (role_id, role) in self.roles {
            for action in role.allowed_actions() {
                if let Some(step_id) = action.step_id() {
                    if !self.steps.contains_key(step_id) {
                        findings.push(CeremonyValidationFinding::error(
                            CeremonyValidationLocus::role(role_id.clone()),
                            DomainError::NotFound {
                                what: "ceremony_role.step_action",
                            },
                        ));
                    }
                }
                if let Some(trigger) = action.transition_trigger() {
                    if !transition_triggers.contains(trigger) {
                        findings.push(CeremonyValidationFinding::error(
                            CeremonyValidationLocus::role(role_id.clone()),
                            DomainError::NotFound {
                                what: "ceremony_role.transition_action",
                            },
                        ));
                    }
                }
            }
        }
    }

    /// Reachability defects are reported as warnings.
    ///
    /// They describe a ceremony that can stall rather than one that is
    /// structurally impossible, and promoting them to errors would
    /// reject definitions that construct today. Whether any of them
    /// graduates to an error is a separate, deliberate decision.
    ///
    /// Analysis is skipped when the graph is not sound enough to walk:
    /// structural errors are reported first and reachability noise on
    /// top of them helps nobody.
    fn collect_reachability_findings(&self, findings: &mut Vec<CeremonyValidationFinding>) {
        let Some(initial) = self.sole_initial_state_id() else {
            return;
        };
        if !self.transition_endpoints_resolve() {
            return;
        }

        if !self.states.values().any(CeremonyState::is_terminal) {
            findings.push(CeremonyValidationFinding::warning(
                CeremonyValidationLocus::Definition,
                DomainError::InvariantViolated {
                    reason: "ceremony definition has no terminal state",
                },
            ));
            return;
        }

        let reachable = self.states_reachable_from(initial);
        let can_finish = self.states_that_reach_a_terminal();
        for state_id in self.states.keys() {
            if !reachable.contains(state_id) {
                findings.push(CeremonyValidationFinding::warning(
                    CeremonyValidationLocus::state(state_id.clone()),
                    DomainError::InvariantViolated {
                        reason: "ceremony state is unreachable from the initial state",
                    },
                ));
            } else if !can_finish.contains(state_id) {
                findings.push(CeremonyValidationFinding::warning(
                    CeremonyValidationLocus::state(state_id.clone()),
                    DomainError::InvariantViolated {
                        reason: "no terminal state is reachable from this ceremony state",
                    },
                ));
            }
        }
    }

    fn sole_initial_state_id(&self) -> Option<&StateId> {
        let mut initial_states = self.states.iter().filter(|(_, state)| state.is_initial());
        let (state_id, _) = initial_states.next()?;
        match initial_states.next() {
            Some(_) => None,
            None => Some(state_id),
        }
    }

    fn transition_endpoints_resolve(&self) -> bool {
        self.transitions.iter().all(|transition| {
            self.states.contains_key(transition.from()) && self.states.contains_key(transition.to())
        })
    }

    fn states_reachable_from(&self, initial: &StateId) -> BTreeSet<StateId> {
        let mut reached = BTreeSet::new();
        let mut pending = vec![initial.clone()];
        while let Some(current) = pending.pop() {
            if !reached.insert(current.clone()) {
                continue;
            }
            for transition in self
                .transitions
                .iter()
                .filter(|transition| transition.from() == &current)
            {
                pending.push(transition.to().clone());
            }
        }
        reached
    }

    fn states_that_reach_a_terminal(&self) -> BTreeSet<StateId> {
        let mut can_finish = self
            .states
            .iter()
            .filter(|(_, state)| state.is_terminal())
            .map(|(state_id, _)| state_id.clone())
            .collect::<BTreeSet<_>>();

        let mut grew = true;
        while grew {
            grew = false;
            for transition in self.transitions {
                if can_finish.contains(transition.to()) && !can_finish.contains(transition.from()) {
                    can_finish.insert(transition.from().clone());
                    grew = true;
                }
            }
        }
        can_finish
    }
}
