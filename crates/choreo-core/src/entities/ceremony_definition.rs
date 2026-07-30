//! [`CeremonyDefinition`] aggregate.
//!
//! A ceremony definition is the declarative state machine extracted
//! from the original laboratory ceremony engine. It is intentionally
//! pure domain: no YAML, no transport, no handler registry.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::value_objects::{
    CeremonyContext, CeremonyDefinitionDigest, CeremonyDescription, CeremonyGuard,
    CeremonyInputDefinition, CeremonyName, CeremonyOutputDefinition, CeremonyRole, CeremonyState,
    CeremonyStep, CeremonyTransition, CeremonyValidationReport, CeremonyVersion, GuardName,
    InputName, OutputName, RoleAction, RoleId, StateId, StepExecutionRecord, StepId,
    TransitionTrigger,
};

use super::ceremony_definition_analysis::CeremonyDefinitionParts;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyDefinition {
    name: CeremonyName,
    version: CeremonyVersion,
    description: Option<CeremonyDescription>,
    inputs: BTreeMap<InputName, CeremonyInputDefinition>,
    outputs: BTreeMap<OutputName, CeremonyOutputDefinition>,
    states: BTreeMap<StateId, CeremonyState>,
    transitions: Vec<CeremonyTransition>,
    steps: BTreeMap<StepId, CeremonyStep>,
    step_order: Vec<StepId>,
    guards: BTreeMap<GuardName, CeremonyGuard>,
    roles: BTreeMap<RoleId, CeremonyRole>,
}

impl CeremonyDefinition {
    pub fn new(
        name: CeremonyName,
        version: CeremonyVersion,
        description: Option<CeremonyDescription>,
        inputs: impl IntoIterator<Item = CeremonyInputDefinition>,
        outputs: impl IntoIterator<Item = CeremonyOutputDefinition>,
        states: impl IntoIterator<Item = CeremonyState>,
        transitions: impl IntoIterator<Item = CeremonyTransition>,
        steps: impl IntoIterator<Item = CeremonyStep>,
        guards: impl IntoIterator<Item = CeremonyGuard>,
        roles: impl IntoIterator<Item = CeremonyRole>,
    ) -> Result<Self, DomainError> {
        let inputs = collect_inputs(inputs)?;
        let outputs = collect_outputs(outputs)?;
        let states = collect_states(states)?;
        let transitions = transitions.into_iter().collect::<Vec<_>>();
        let (steps, step_order) = collect_steps(steps)?;
        let guards = collect_guards(guards)?;
        let roles = collect_roles(roles)?;

        let definition = Self {
            name,
            version,
            description,
            inputs,
            outputs,
            states,
            transitions,
            steps,
            step_order,
            guards,
            roles,
        };
        definition.validate()?;
        Ok(definition)
    }

    #[must_use]
    pub fn name(&self) -> &CeremonyName {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> &CeremonyVersion {
        &self.version
    }

    #[must_use]
    pub fn description(&self) -> Option<&CeremonyDescription> {
        self.description.as_ref()
    }

    #[must_use]
    pub fn inputs(&self) -> &BTreeMap<InputName, CeremonyInputDefinition> {
        &self.inputs
    }

    #[must_use]
    pub fn outputs(&self) -> &BTreeMap<OutputName, CeremonyOutputDefinition> {
        &self.outputs
    }

    #[must_use]
    pub fn states(&self) -> &BTreeMap<StateId, CeremonyState> {
        &self.states
    }

    #[must_use]
    pub fn transitions(&self) -> &[CeremonyTransition] {
        &self.transitions
    }

    #[must_use]
    pub fn steps(&self) -> &BTreeMap<StepId, CeremonyStep> {
        &self.steps
    }

    /// Iterate over every step in its declaration order.
    ///
    /// Step identifiers remain indexed separately for efficient lookup;
    /// execution order is an explicit part of the ceremony definition.
    pub fn steps_in_declaration_order(&self) -> impl Iterator<Item = &CeremonyStep> + '_ {
        self.step_order.iter().map(|step_id| {
            self.steps
                .get(step_id)
                .expect("ceremony step order must reference an indexed step")
        })
    }

    #[must_use]
    pub fn guards(&self) -> &BTreeMap<GuardName, CeremonyGuard> {
        &self.guards
    }

    #[must_use]
    pub fn roles(&self) -> &BTreeMap<RoleId, CeremonyRole> {
        &self.roles
    }

    #[must_use]
    pub fn initial_state_id(&self) -> &StateId {
        self.states
            .values()
            .find(|state| state.is_initial())
            .map(CeremonyState::id)
            .expect("ceremony definition invariant requires one initial state")
    }

    #[must_use]
    pub fn state(&self, state_id: &StateId) -> Option<&CeremonyState> {
        self.states.get(state_id)
    }

    #[must_use]
    pub fn step(&self, step_id: &StepId) -> Option<&CeremonyStep> {
        self.steps.get(step_id)
    }

    #[must_use]
    pub fn role(&self, role_id: &RoleId) -> Option<&CeremonyRole> {
        self.roles.get(role_id)
    }

    pub fn steps_for_state(&self, state_id: &StateId) -> impl Iterator<Item = &CeremonyStep> + '_ {
        let state_id = state_id.clone();
        self.steps_in_declaration_order()
            .filter(move |step| step.state_id() == &state_id)
    }

    #[must_use]
    pub fn is_terminal_state(&self, state_id: &StateId) -> bool {
        self.states
            .get(state_id)
            .is_some_and(CeremonyState::is_terminal)
    }

    #[must_use]
    pub fn transition_for_trigger(
        &self,
        state_id: &StateId,
        trigger: &TransitionTrigger,
    ) -> Option<&CeremonyTransition> {
        self.transitions
            .iter()
            .find(|transition| transition.from() == state_id && transition.trigger() == trigger)
    }

    pub fn available_transitions(
        &self,
        state_id: &StateId,
    ) -> impl Iterator<Item = &CeremonyTransition> + '_ {
        let state_id = state_id.clone();
        self.transitions
            .iter()
            .filter(move |transition| transition.from() == &state_id)
    }

    #[must_use]
    pub fn role_allows(&self, role_id: &RoleId, action: &RoleAction) -> bool {
        self.roles
            .get(role_id)
            .is_some_and(|role| role.allows(action))
    }

    #[must_use]
    pub fn guards_are_satisfied(
        &self,
        transition: &CeremonyTransition,
        records: &BTreeMap<StepId, StepExecutionRecord>,
        context: &CeremonyContext,
    ) -> bool {
        transition.required_guards().iter().all(|guard_name| {
            self.guards
                .get(guard_name)
                .is_some_and(|guard| guard.is_satisfied(records, context))
        })
    }

    /// Find the role authorised to perform `action`, if any.
    ///
    /// Roles are scanned in id order and the first whose action set
    /// permits `action` is returned. Yields `None` when no declared role
    /// is allowed to perform it.
    #[must_use]
    pub fn role_for_action(&self, action: &RoleAction) -> Option<&CeremonyRole> {
        self.roles.values().find(|role| role.allows(action))
    }

    /// Resolve the role authorised to execute `step_id`.
    ///
    /// Fails fast with [`DomainError::InvariantViolated`] when no role is
    /// allowed to run the step — a ceremony cannot execute a step nobody
    /// owns.
    pub fn role_id_for_step(&self, step_id: &StepId) -> Result<RoleId, DomainError> {
        self.role_for_action(&RoleAction::step(step_id.clone()))
            .map(|role| role.id().clone())
            .ok_or(DomainError::InvariantViolated {
                reason: "no ceremony role can execute step",
            })
    }

    /// Resolve the role authorised to apply the transition fired by
    /// `trigger`.
    ///
    /// Fails fast with [`DomainError::InvariantViolated`] when no role is
    /// allowed to apply it — a ceremony cannot advance through a
    /// transition nobody owns.
    pub fn role_id_for_transition(
        &self,
        trigger: &TransitionTrigger,
    ) -> Result<RoleId, DomainError> {
        self.role_for_action(&RoleAction::transition(trigger.clone()))
            .map(|role| role.id().clone())
            .ok_or(DomainError::InvariantViolated {
                reason: "no ceremony role can apply transition",
            })
    }

    /// Select the next transition out of `state_id` whose guards are all
    /// currently satisfied by `records` and `context`.
    ///
    /// Outgoing transitions are evaluated in declaration order and the
    /// first one that is fully enabled is returned. Yields `None` when
    /// the state has no outgoing transition whose guards hold — either
    /// because the state is terminal or because the ceremony is not yet
    /// ready to advance.
    #[must_use]
    pub fn next_satisfied_transition(
        &self,
        state_id: &StateId,
        records: &BTreeMap<StepId, StepExecutionRecord>,
        context: &CeremonyContext,
    ) -> Option<&CeremonyTransition> {
        self.available_transitions(state_id)
            .find(|transition| self.guards_are_satisfied(transition, records, context))
    }

    /// The identity of this definition's content.
    ///
    /// Computed over canonical JSON of the whole aggregate rather than
    /// over the document it arrived in: two YAML files differing in
    /// whitespace, key order or comments describe the same working
    /// session and must agree, while any material difference must not.
    ///
    /// Encoding the aggregate through `serde` rather than by hand is
    /// deliberate. A hand-written encoder that forgets a field produces
    /// two materially different definitions with one digest, and
    /// nothing would report it; here a field cannot be left out, and a
    /// field added later changes the digest, which is correct because
    /// it is material.
    ///
    /// Canonical because `serde_json` maps are ordered — see
    /// `serde_json_emits_sorted_keys` for the guard that keeps that
    /// assumption from being silently withdrawn.
    pub fn digest(&self) -> Result<CeremonyDefinitionDigest, DomainError> {
        let canonical = serde_json::to_vec(self).map_err(|_| DomainError::InvariantViolated {
            reason: "ceremony definition cannot be rendered canonically",
        })?;
        Ok(CeremonyDefinitionDigest::of_canonical_form(&canonical))
    }

    /// Collect every defect in the definition instead of stopping at
    /// the first one.
    ///
    /// A single error is enough to reject a definition but not enough
    /// to correct one. An author — human or agent — needs the full set
    /// to fix a draft in one pass.
    ///
    /// Findings are emitted in check order, so the first blocking one
    /// is exactly the error [`Self::new`] raises.
    #[must_use]
    pub fn analyze(&self) -> CeremonyValidationReport {
        let mut findings = Vec::new();
        self.parts().collect_findings(&mut findings);
        CeremonyValidationReport::new(findings)
    }

    fn parts(&self) -> CeremonyDefinitionParts<'_> {
        CeremonyDefinitionParts {
            states: &self.states,
            transitions: &self.transitions,
            steps: &self.steps,
            guards: &self.guards,
            roles: &self.roles,
        }
    }

    fn validate(&self) -> Result<(), DomainError> {
        match self.analyze().first_error() {
            Some(finding) => Err(finding.defect().clone()),
            None => Ok(()),
        }
    }
}

fn collect_inputs(
    inputs: impl IntoIterator<Item = CeremonyInputDefinition>,
) -> Result<BTreeMap<InputName, CeremonyInputDefinition>, DomainError> {
    let mut map = BTreeMap::new();
    for input in inputs {
        if map.insert(input.name().clone(), input).is_some() {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_input",
            });
        }
    }
    Ok(map)
}

fn collect_outputs(
    outputs: impl IntoIterator<Item = CeremonyOutputDefinition>,
) -> Result<BTreeMap<OutputName, CeremonyOutputDefinition>, DomainError> {
    let mut map = BTreeMap::new();
    for output in outputs {
        if map.insert(output.name().clone(), output).is_some() {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_output",
            });
        }
    }
    Ok(map)
}

fn collect_states(
    states: impl IntoIterator<Item = CeremonyState>,
) -> Result<BTreeMap<StateId, CeremonyState>, DomainError> {
    let mut map = BTreeMap::new();
    for state in states {
        if map.insert(state.id().clone(), state).is_some() {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_state",
            });
        }
    }
    Ok(map)
}

fn collect_steps(
    steps: impl IntoIterator<Item = CeremonyStep>,
) -> Result<(BTreeMap<StepId, CeremonyStep>, Vec<StepId>), DomainError> {
    let mut map = BTreeMap::new();
    let mut order = Vec::new();
    for step in steps {
        let step_id = step.id().clone();
        if map.insert(step_id.clone(), step).is_some() {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_step",
            });
        }
        order.push(step_id);
    }
    Ok((map, order))
}

fn collect_guards(
    guards: impl IntoIterator<Item = CeremonyGuard>,
) -> Result<BTreeMap<GuardName, CeremonyGuard>, DomainError> {
    let mut map = BTreeMap::new();
    for guard in guards {
        if map.insert(guard.name().clone(), guard).is_some() {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_guard",
            });
        }
    }
    Ok(map)
}

fn collect_roles(
    roles: impl IntoIterator<Item = CeremonyRole>,
) -> Result<BTreeMap<RoleId, CeremonyRole>, DomainError> {
    let mut map = BTreeMap::new();
    for role in roles {
        if map.insert(role.id().clone(), role).is_some() {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_role",
            });
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::{
        CeremonyStateKind, CeremonyValidationLocus, GuardCondition, RetryPolicy, StepHandlerConfig,
        StepHandlerKind, StepStatus,
    };

    fn name() -> CeremonyName {
        CeremonyName::new("planning_ceremony").unwrap()
    }

    fn state_id(raw: &str) -> StateId {
        StateId::new(raw).unwrap()
    }

    fn step_id(raw: &str) -> StepId {
        StepId::new(raw).unwrap()
    }

    fn guard_name(raw: &str) -> GuardName {
        GuardName::new(raw).unwrap()
    }

    fn trigger(raw: &str) -> TransitionTrigger {
        TransitionTrigger::new(raw).unwrap()
    }

    fn handler_kind() -> StepHandlerKind {
        StepHandlerKind::new("manual_review").unwrap()
    }

    fn step(raw_step_id: &str, raw_state_id: &str) -> CeremonyStep {
        CeremonyStep::new(
            step_id(raw_step_id),
            state_id(raw_state_id),
            handler_kind(),
            StepHandlerConfig::empty(),
            RetryPolicy::single_attempt(),
            None,
        )
    }

    fn role(actions: Vec<RoleAction>) -> CeremonyRole {
        CeremonyRole::new(RoleId::new("facilitator").unwrap(), actions).unwrap()
    }

    fn definition(
        states: Vec<CeremonyState>,
        transitions: Vec<CeremonyTransition>,
        steps: Vec<CeremonyStep>,
        guards: Vec<CeremonyGuard>,
        roles: Vec<CeremonyRole>,
    ) -> Result<CeremonyDefinition, DomainError> {
        CeremonyDefinition::new(
            name(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            states,
            transitions,
            steps,
            guards,
            roles,
        )
    }

    fn valid_definition() -> CeremonyDefinition {
        let plan_step = step("plan", "drafting");
        let guard = CeremonyGuard::new(
            guard_name("plan_done"),
            GuardCondition::StepStatus {
                step_id: plan_step.id().clone(),
                status: StepStatus::Completed,
            },
        );
        let transition = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("finish"),
            vec![guard.name().clone()],
        )
        .unwrap();
        let role = role(vec![
            RoleAction::step(plan_step.id().clone()),
            RoleAction::transition(transition.trigger().clone()),
        ]);

        definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![transition],
            vec![plan_step],
            vec![guard],
            vec![role],
        )
        .unwrap()
    }

    #[test]
    fn accepts_valid_declarative_state_machine() {
        let definition = valid_definition();

        assert_eq!(definition.initial_state_id(), &state_id("drafting"));
        assert_eq!(definition.steps_for_state(&state_id("drafting")).count(), 1);
        assert!(definition.role_allows(
            &RoleId::new("facilitator").unwrap(),
            &RoleAction::transition(trigger("finish"))
        ));
    }

    #[test]
    fn preserves_step_declaration_order_within_a_state() {
        let definition = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            vec![
                step("write_plan", "drafting"),
                step("challenge_plan", "drafting"),
                step("archive_plan", "drafting"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let step_ids = definition
            .steps_for_state(&state_id("drafting"))
            .map(CeremonyStep::id)
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(
            step_ids,
            vec![
                step_id("write_plan"),
                step_id("challenge_plan"),
                step_id("archive_plan"),
            ]
        );
    }

    #[test]
    fn rejects_definitions_without_exactly_one_initial_state() {
        let err = definition(
            vec![
                CeremonyState::new(state_id("one"), CeremonyStateKind::Initial),
                CeremonyState::new(state_id("two"), CeremonyStateKind::Initial),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn rejects_terminal_states_with_outgoing_transitions() {
        let transition = CeremonyTransition::new(
            state_id("done"),
            state_id("drafting"),
            trigger("restart"),
            Vec::new(),
        )
        .unwrap();

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![transition],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn rejects_roles_that_reference_unknown_steps() {
        let role = role(vec![RoleAction::step(step_id("missing"))]);

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![role],
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_role.step_action"
            }
        ));
    }

    #[test]
    fn rejects_empty_states_collection() {
        let err =
            definition(Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()).unwrap_err();

        assert!(matches!(
            err,
            DomainError::EmptyCollection {
                field: "ceremony_definition.states"
            }
        ));
    }

    #[test]
    fn rejects_transition_referencing_unknown_from_state() {
        let transition = CeremonyTransition::new(
            state_id("ghost"),
            state_id("done"),
            trigger("finish"),
            Vec::new(),
        )
        .unwrap();

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![transition],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_transition.from_state"
            }
        ));
    }

    #[test]
    fn rejects_transition_referencing_unknown_to_state() {
        let transition = CeremonyTransition::new(
            state_id("drafting"),
            state_id("ghost"),
            trigger("finish"),
            Vec::new(),
        )
        .unwrap();

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![transition],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_transition.to_state"
            }
        ));
    }

    #[test]
    fn rejects_duplicate_state_trigger_pairs() {
        let first = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("finish"),
            Vec::new(),
        )
        .unwrap();
        let duplicate = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("finish"),
            Vec::new(),
        )
        .unwrap();

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![first, duplicate],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::AlreadyExists {
                what: "ceremony_transition.state_trigger"
            }
        ));
    }

    #[test]
    fn rejects_transition_referencing_unknown_guard() {
        let transition = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("finish"),
            vec![guard_name("absent")],
        )
        .unwrap();

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![transition],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_transition.guard"
            }
        ));
    }

    #[test]
    fn rejects_step_referencing_unknown_state() {
        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            vec![step("plan", "ghost")],
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_step.state"
            }
        ));
    }

    #[test]
    fn rejects_role_that_references_unknown_transition_trigger() {
        let role = role(vec![RoleAction::transition(trigger("ghost"))]);

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![role],
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_role.transition_action"
            }
        ));
    }

    #[test]
    fn resolves_role_authorised_for_a_step() {
        let definition = valid_definition();

        assert_eq!(
            definition.role_id_for_step(&step_id("plan")).unwrap(),
            RoleId::new("facilitator").unwrap()
        );
    }

    #[test]
    fn rejects_step_with_no_authorised_role() {
        let definition = valid_definition();

        let err = definition
            .role_id_for_step(&step_id("unowned"))
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "no ceremony role can execute step"
            }
        ));
    }

    #[test]
    fn resolves_role_authorised_for_a_transition() {
        let definition = valid_definition();

        assert_eq!(
            definition
                .role_id_for_transition(&trigger("finish"))
                .unwrap(),
            RoleId::new("facilitator").unwrap()
        );
    }

    #[test]
    fn rejects_transition_with_no_authorised_role() {
        let definition = valid_definition();

        let err = definition
            .role_id_for_transition(&trigger("unowned"))
            .unwrap_err();

        assert!(matches!(
            err,
            DomainError::InvariantViolated {
                reason: "no ceremony role can apply transition"
            }
        ));
    }

    #[test]
    fn selects_guardless_transition_as_immediately_enabled() {
        let transition = CeremonyTransition::new(
            state_id("open"),
            state_id("closed"),
            trigger("go"),
            Vec::new(),
        )
        .unwrap();
        let definition = definition(
            vec![
                CeremonyState::initial(state_id("open")),
                CeremonyState::terminal(state_id("closed")),
            ],
            vec![transition],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let selected = definition
            .next_satisfied_transition(
                &state_id("open"),
                &BTreeMap::new(),
                &CeremonyContext::empty(),
            )
            .expect("guardless transition is always enabled");

        assert_eq!(selected.trigger(), &trigger("go"));
    }

    #[test]
    fn skips_transition_whose_guards_are_unsatisfied() {
        let definition = valid_definition();

        assert!(definition
            .next_satisfied_transition(
                &state_id("drafting"),
                &BTreeMap::new(),
                &CeremonyContext::empty(),
            )
            .is_none());
    }

    #[test]
    fn yields_no_transition_out_of_a_terminal_state() {
        let definition = valid_definition();

        assert!(definition
            .next_satisfied_transition(
                &state_id("done"),
                &BTreeMap::new(),
                &CeremonyContext::empty(),
            )
            .is_none());
    }

    #[test]
    fn rejects_guards_that_reference_unknown_steps() {
        let guard = CeremonyGuard::new(
            guard_name("unknown_step_done"),
            GuardCondition::StepStatus {
                step_id: step_id("missing"),
                status: StepStatus::Completed,
            },
        );

        let err = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            Vec::new(),
            vec![guard],
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "ceremony_guard.step"
            }
        ));
    }

    /// The digest is canonical only because `serde_json` orders object
    /// keys. Enabling `preserve_order` anywhere in the dependency graph
    /// — including through feature unification by a crate nobody here
    /// chose — would withdraw that silently and change every digest.
    /// This is what makes it loud instead.
    #[test]
    fn serde_json_emits_sorted_keys() {
        let mut out_of_order = serde_json::Map::new();
        out_of_order.insert("zulu".to_owned(), serde_json::Value::from(1));
        out_of_order.insert("alpha".to_owned(), serde_json::Value::from(2));

        assert_eq!(
            serde_json::to_string(&serde_json::Value::Object(out_of_order)).unwrap(),
            r#"{"alpha":2,"zulu":1}"#,
            "serde_json is no longer emitting sorted keys, so the definition digest is not canonical"
        );
    }

    #[test]
    fn the_same_definition_always_digests_the_same() {
        assert_eq!(
            valid_definition().digest().unwrap(),
            valid_definition().digest().unwrap()
        );
    }

    #[test]
    fn a_material_difference_changes_the_digest() {
        let baseline = valid_definition().digest().unwrap();
        let renamed = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("finished")),
            ],
            vec![CeremonyTransition::new(
                state_id("drafting"),
                state_id("finished"),
                trigger("finish"),
                Vec::new(),
            )
            .unwrap()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
        .digest()
        .unwrap();

        assert_ne!(baseline, renamed);
    }

    #[test]
    fn transition_order_is_material_to_the_digest() {
        // Declaration order decides which transition fires first when
        // several are enabled, so two definitions that differ only in
        // that order are different working sessions.
        let states = || {
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
                CeremonyState::terminal(state_id("cancelled")),
            ]
        };
        let finish = || {
            CeremonyTransition::new(
                state_id("drafting"),
                state_id("done"),
                trigger("finish"),
                Vec::new(),
            )
            .unwrap()
        };
        let cancel = || {
            CeremonyTransition::new(
                state_id("drafting"),
                state_id("cancelled"),
                trigger("cancel"),
                Vec::new(),
            )
            .unwrap()
        };

        let first = definition(
            states(),
            vec![finish(), cancel()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let swapped = definition(
            states(),
            vec![cancel(), finish()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        assert_ne!(first.digest().unwrap(), swapped.digest().unwrap());
    }

    #[test]
    fn a_valid_definition_reports_no_findings_at_all() {
        let report = valid_definition().analyze();

        assert!(report.is_valid());
        assert!(report.findings().is_empty());
    }

    #[test]
    fn an_unreachable_state_is_warned_about_without_blocking_construction() {
        let definition = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::intermediate(state_id("orphan")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![
                CeremonyTransition::new(
                    state_id("drafting"),
                    state_id("done"),
                    trigger("finish"),
                    Vec::new(),
                )
                .unwrap(),
                CeremonyTransition::new(
                    state_id("orphan"),
                    state_id("done"),
                    trigger("rescue"),
                    Vec::new(),
                )
                .unwrap(),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let report = definition.analyze();
        let warnings = report.warnings().collect::<Vec<_>>();

        assert!(report.is_valid());
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].locus(),
            &CeremonyValidationLocus::state(state_id("orphan"))
        );
    }

    #[test]
    fn a_state_that_cannot_reach_a_terminal_is_warned_about() {
        let definition = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::intermediate(state_id("stuck")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![
                CeremonyTransition::new(
                    state_id("drafting"),
                    state_id("done"),
                    trigger("finish"),
                    Vec::new(),
                )
                .unwrap(),
                CeremonyTransition::new(
                    state_id("drafting"),
                    state_id("stuck"),
                    trigger("stall"),
                    Vec::new(),
                )
                .unwrap(),
                CeremonyTransition::new(
                    state_id("stuck"),
                    state_id("stuck"),
                    trigger("spin"),
                    Vec::new(),
                )
                .unwrap(),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let report = definition.analyze();
        let warnings = report.warnings().collect::<Vec<_>>();

        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].locus(),
            &CeremonyValidationLocus::state(state_id("stuck"))
        );
    }

    #[test]
    fn a_definition_without_any_terminal_state_is_warned_about() {
        let definition = definition(
            vec![CeremonyState::initial(state_id("drafting"))],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let report = definition.analyze();
        let warnings = report.warnings().collect::<Vec<_>>();

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].locus(), &CeremonyValidationLocus::Definition);
    }

    #[test]
    fn structural_errors_suppress_reachability_noise() {
        let report = definition(
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::initial(state_id("also_drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_err();

        assert!(matches!(
            report,
            DomainError::InvariantViolated {
                reason: "ceremony definition must have exactly one initial state"
            }
        ));
    }
}
