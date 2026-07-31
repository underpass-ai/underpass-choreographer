use crate::entities::CeremonyDefinition;

use super::{
    CeremonyInputDefinition, CeremonyValidationLocus, InputRequirement, StateId, TransitionTrigger,
};

fn is_required(input: &CeremonyInputDefinition) -> bool {
    matches!(input.requirement(), InputRequirement::Required)
}

/// What happened to one element between two definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CeremonyChangeKind {
    Added,
    Removed,
    Altered,
}

impl CeremonyChangeKind {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Altered => "altered",
        }
    }
}

/// Whether a session already running the earlier definition could go
/// on if it were pointed at the later one.
///
/// This is the question a diff is asked in practice. "What changed" is
/// answerable by reading both documents; "can the meeting that is
/// happening right now still finish" is not, and it is the one that
/// decides whether a new version may be adopted mid-flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CeremonyChangeImpact {
    /// A running session is unaffected.
    Carries,
    /// A running session could be left with nowhere to go.
    Strands,
}

impl CeremonyChangeImpact {
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Carries => "carries",
            Self::Strands => "strands",
        }
    }

    #[must_use]
    pub const fn strands(self) -> bool {
        matches!(self, Self::Strands)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeremonyDefinitionChange {
    kind: CeremonyChangeKind,
    locus: CeremonyValidationLocus,
    impact: CeremonyChangeImpact,
    detail: &'static str,
}

impl CeremonyDefinitionChange {
    #[must_use]
    pub const fn kind(&self) -> CeremonyChangeKind {
        self.kind
    }

    #[must_use]
    pub const fn locus(&self) -> &CeremonyValidationLocus {
        &self.locus
    }

    #[must_use]
    pub const fn impact(&self) -> CeremonyChangeImpact {
        self.impact
    }

    /// What changed about the element, in words. The locus says which
    /// element; this says what about it.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

/// Everything that differs between two definitions of one ceremony.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CeremonyDefinitionDiff {
    changes: Vec<CeremonyDefinitionChange>,
}

impl CeremonyDefinitionDiff {
    /// Compare two definitions, earlier first.
    ///
    /// The two need not be versions of the same ceremony: comparing
    /// unrelated documents is a strange thing to ask for but not a
    /// wrong one, and refusing it would mean deciding on the caller's
    /// behalf what counts as related.
    #[must_use]
    pub fn between(before: &CeremonyDefinition, after: &CeremonyDefinition) -> Self {
        let mut changes = Vec::new();
        diff_states(before, after, &mut changes);
        diff_transitions(before, after, &mut changes);
        diff_steps(before, after, &mut changes);
        diff_guards(before, after, &mut changes);
        diff_roles(before, after, &mut changes);
        diff_shape(before, after, &mut changes);
        Self { changes }
    }

    #[must_use]
    pub fn changes(&self) -> &[CeremonyDefinitionChange] {
        &self.changes
    }

    #[must_use]
    pub fn is_identical(&self) -> bool {
        self.changes.is_empty()
    }

    /// Whether any single change could strand a running session. One is
    /// enough: a version that can strand a session is not one to adopt
    /// mid-flight, however much else about it is harmless.
    #[must_use]
    pub fn strands_running_sessions(&self) -> bool {
        self.changes.iter().any(|change| change.impact.strands())
    }

    #[must_use]
    pub fn strand_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.impact.strands())
            .count()
    }
}

fn record(
    changes: &mut Vec<CeremonyDefinitionChange>,
    kind: CeremonyChangeKind,
    locus: CeremonyValidationLocus,
    impact: CeremonyChangeImpact,
    detail: &'static str,
) {
    changes.push(CeremonyDefinitionChange {
        kind,
        locus,
        impact,
        detail,
    });
}

fn diff_states(
    before: &CeremonyDefinition,
    after: &CeremonyDefinition,
    changes: &mut Vec<CeremonyDefinitionChange>,
) {
    for (id, state) in before.states() {
        match after.states().get(id) {
            // A session sitting in a state that no longer exists has
            // nowhere to be.
            None => record(
                changes,
                CeremonyChangeKind::Removed,
                CeremonyValidationLocus::state(id.clone()),
                CeremonyChangeImpact::Strands,
                "a session in this state would have nowhere to be",
            ),
            Some(now) if now.kind() != state.kind() => record(
                changes,
                CeremonyChangeKind::Altered,
                CeremonyValidationLocus::state(id.clone()),
                // Becoming terminal ends sessions early; ceasing to be
                // terminal leaves finished ones unfinished.
                CeremonyChangeImpact::Strands,
                "whether the session may start or finish here",
            ),
            Some(_) => {}
        }
    }
    for id in after.states().keys() {
        if !before.states().contains_key(id) {
            record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::state(id.clone()),
                CeremonyChangeImpact::Carries,
                "a state no running session is in yet",
            );
        }
    }
}

fn transition_key(from: &StateId, trigger: &TransitionTrigger) -> (String, String) {
    (from.as_str().to_owned(), trigger.as_str().to_owned())
}

fn diff_transitions(
    before: &CeremonyDefinition,
    after: &CeremonyDefinition,
    changes: &mut Vec<CeremonyDefinitionChange>,
) {
    for old in before.transitions() {
        let key = transition_key(old.from(), old.trigger());
        let now = after
            .transitions()
            .iter()
            .find(|candidate| transition_key(candidate.from(), candidate.trigger()) == key);
        let locus = CeremonyValidationLocus::transition(old.from().clone(), old.trigger().clone());
        match now {
            // A session may have been counting on exactly this move.
            None => record(
                changes,
                CeremonyChangeKind::Removed,
                locus,
                CeremonyChangeImpact::Strands,
                "a way out of the state it leaves",
            ),
            Some(now) if now.to() != old.to() => record(
                changes,
                CeremonyChangeKind::Altered,
                locus,
                CeremonyChangeImpact::Strands,
                "where it leads",
            ),
            Some(now) if now.required_guards() != old.required_guards() => {
                // Guards added can block a session that was about to
                // move; guards dropped only ever let it through.
                let impact = if now.required_guards().is_superset(old.required_guards()) {
                    CeremonyChangeImpact::Strands
                } else {
                    CeremonyChangeImpact::Carries
                };
                record(
                    changes,
                    CeremonyChangeKind::Altered,
                    locus,
                    impact,
                    "what has to hold before it can fire",
                );
            }
            Some(_) => {}
        }
    }
    for now in after.transitions() {
        let key = transition_key(now.from(), now.trigger());
        if !before
            .transitions()
            .iter()
            .any(|candidate| transition_key(candidate.from(), candidate.trigger()) == key)
        {
            record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::transition(now.from().clone(), now.trigger().clone()),
                CeremonyChangeImpact::Carries,
                "another way out of a state",
            );
        }
    }
}

fn diff_steps(
    before: &CeremonyDefinition,
    after: &CeremonyDefinition,
    changes: &mut Vec<CeremonyDefinitionChange>,
) {
    for (id, old) in before.steps() {
        let locus = CeremonyValidationLocus::step(id.clone());
        let Some(now) = after.steps().get(id) else {
            // A guard that waits on this step will wait forever, and a
            // session that has not run it never will.
            record(
                changes,
                CeremonyChangeKind::Removed,
                locus,
                CeremonyChangeImpact::Strands,
                "work a session may not have done yet",
            );
            continue;
        };
        if now.state_id() != old.state_id() {
            record(
                changes,
                CeremonyChangeKind::Altered,
                locus.clone(),
                CeremonyChangeImpact::Strands,
                "the state it belongs to",
            );
        }
        if now.handler_kind() != old.handler_kind() {
            record(
                changes,
                CeremonyChangeKind::Altered,
                locus.clone(),
                CeremonyChangeImpact::Carries,
                "who does the work",
            );
        }
        if now.handler_config() != old.handler_config() {
            record(
                changes,
                CeremonyChangeKind::Altered,
                locus.clone(),
                CeremonyChangeImpact::Carries,
                "how the work is asked for",
            );
        }
        if now.retry_policy() != old.retry_policy() || now.timeout() != old.timeout() {
            record(
                changes,
                CeremonyChangeKind::Altered,
                locus,
                CeremonyChangeImpact::Carries,
                "how long it may take and how often it may be retried",
            );
        }
    }
    for id in after.steps().keys() {
        if !before.steps().contains_key(id) {
            // Added work is not harmless: a session already past this
            // step's state will never run it, and a guard waiting on
            // it would never be satisfied.
            record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::step(id.clone()),
                CeremonyChangeImpact::Strands,
                "work a session may already have moved past",
            );
        }
    }
}

fn diff_guards(
    before: &CeremonyDefinition,
    after: &CeremonyDefinition,
    changes: &mut Vec<CeremonyDefinitionChange>,
) {
    for (name, old) in before.guards() {
        let locus = CeremonyValidationLocus::guard(name.clone());
        match after.guards().get(name) {
            None => record(
                changes,
                CeremonyChangeKind::Removed,
                locus,
                CeremonyChangeImpact::Carries,
                "a condition no longer asked for",
            ),
            // An automated guard turned human now waits on a person who
            // was never asked; a human guard turned automated discards
            // an approval already given.
            Some(now) if now.condition() != old.condition() => record(
                changes,
                CeremonyChangeKind::Altered,
                locus,
                CeremonyChangeImpact::Strands,
                "what satisfies it",
            ),
            Some(_) => {}
        }
    }
    for name in after.guards().keys() {
        if !before.guards().contains_key(name) {
            record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::guard(name.clone()),
                CeremonyChangeImpact::Carries,
                "a condition that only matters where a transition asks for it",
            );
        }
    }
}

fn diff_roles(
    before: &CeremonyDefinition,
    after: &CeremonyDefinition,
    changes: &mut Vec<CeremonyDefinitionChange>,
) {
    for (id, old) in before.roles() {
        let locus = CeremonyValidationLocus::role(id.clone());
        match after.roles().get(id) {
            None => record(
                changes,
                CeremonyChangeKind::Removed,
                locus,
                CeremonyChangeImpact::Strands,
                "whoever was acting as this role can no longer act",
            ),
            Some(now) if now.allowed_actions() != old.allowed_actions() => {
                // Only narrowing takes something away.
                let impact = if old.allowed_actions().is_subset(now.allowed_actions()) {
                    CeremonyChangeImpact::Carries
                } else {
                    CeremonyChangeImpact::Strands
                };
                record(
                    changes,
                    CeremonyChangeKind::Altered,
                    locus,
                    impact,
                    "what this role is allowed to do",
                );
            }
            Some(_) => {}
        }
    }
    for id in after.roles().keys() {
        if !before.roles().contains_key(id) {
            record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::role(id.clone()),
                CeremonyChangeImpact::Carries,
                "another role at the table",
            );
        }
    }
}

/// Everything that is not the graph: what the ceremony says it needs
/// and what it says it produces.
fn diff_shape(
    before: &CeremonyDefinition,
    after: &CeremonyDefinition,
    changes: &mut Vec<CeremonyDefinitionChange>,
) {
    for name in before.inputs().keys() {
        if !after.inputs().contains_key(name) {
            record(
                changes,
                CeremonyChangeKind::Removed,
                CeremonyValidationLocus::input(name.clone()),
                CeremonyChangeImpact::Carries,
                "an input no longer asked for",
            );
        }
    }
    for (name, now) in after.inputs() {
        match before.inputs().get(name) {
            None => record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::input(name.clone()),
                // A session started without it cannot go back and
                // supply it.
                if is_required(now) {
                    CeremonyChangeImpact::Strands
                } else {
                    CeremonyChangeImpact::Carries
                },
                "an input the ceremony now asks for",
            ),
            Some(was) if is_required(was) != is_required(now) => record(
                changes,
                CeremonyChangeKind::Altered,
                CeremonyValidationLocus::input(name.clone()),
                if is_required(now) {
                    CeremonyChangeImpact::Strands
                } else {
                    CeremonyChangeImpact::Carries
                },
                "whether it has to be supplied",
            ),
            Some(_) => {}
        }
    }
    for name in before.outputs().keys() {
        if !after.outputs().contains_key(name) {
            record(
                changes,
                CeremonyChangeKind::Removed,
                CeremonyValidationLocus::output(name.clone()),
                CeremonyChangeImpact::Carries,
                "an output no longer produced",
            );
        }
    }
    for name in after.outputs().keys() {
        if !before.outputs().contains_key(name) {
            record(
                changes,
                CeremonyChangeKind::Added,
                CeremonyValidationLocus::output(name.clone()),
                CeremonyChangeImpact::Carries,
                "an output not produced before",
            );
        }
    }
    if before.description() != after.description() {
        record(
            changes,
            CeremonyChangeKind::Altered,
            CeremonyValidationLocus::Definition,
            CeremonyChangeImpact::Carries,
            "what the ceremony says it is for",
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::value_objects::{
        CeremonyGuard, CeremonyName, CeremonyRole, CeremonyState, CeremonyStep, CeremonyTransition,
        CeremonyVersion, GuardCondition, GuardName, RetryPolicy, RoleAction, RoleId,
        StepHandlerConfig, StepHandlerKind, StepId,
    };

    fn state(id: &str) -> StateId {
        StateId::new(id).unwrap()
    }

    fn trigger(name: &str) -> TransitionTrigger {
        TransitionTrigger::new(name).unwrap()
    }

    fn guard_name(name: &str) -> GuardName {
        GuardName::new(name).unwrap()
    }

    fn step_id(id: &str) -> StepId {
        StepId::new(id).unwrap()
    }

    fn step(id: &str, in_state: &str, handler: &str) -> CeremonyStep {
        CeremonyStep::new(
            step_id(id),
            state(in_state),
            StepHandlerKind::new(handler).unwrap(),
            StepHandlerConfig::empty(),
            RetryPolicy::default(),
            None,
        )
    }

    fn transition(from: &str, to: &str, on: &str, guards: Vec<GuardName>) -> CeremonyTransition {
        CeremonyTransition::new(state(from), state(to), trigger(on), guards).unwrap()
    }

    fn role(id: &str, actions: BTreeSet<RoleAction>) -> CeremonyRole {
        CeremonyRole::new(RoleId::new(id).unwrap(), actions).unwrap()
    }

    fn facilitator_actions() -> BTreeSet<RoleAction> {
        BTreeSet::from([
            RoleAction::step(step_id("work")),
            RoleAction::transition(trigger("finish")),
        ])
    }

    /// One state to work in, one to finish in, one step, one guard on
    /// the way out, one role allowed to do both.
    struct Draft {
        states: Vec<CeremonyState>,
        transitions: Vec<CeremonyTransition>,
        steps: Vec<CeremonyStep>,
        guards: Vec<CeremonyGuard>,
        roles: Vec<CeremonyRole>,
    }

    impl Draft {
        fn baseline() -> Self {
            Self {
                states: vec![
                    CeremonyState::initial(state("OPEN")),
                    CeremonyState::terminal(state("DONE")),
                ],
                transitions: vec![transition(
                    "OPEN",
                    "DONE",
                    "finish",
                    vec![guard_name("work_done")],
                )],
                steps: vec![step("work", "OPEN", "noop")],
                guards: vec![CeremonyGuard::new(
                    guard_name("work_done"),
                    GuardCondition::AllStepsCompleted,
                )],
                roles: vec![role("FACILITATOR", facilitator_actions())],
            }
        }

        fn build(self) -> CeremonyDefinition {
            CeremonyDefinition::new(
                CeremonyName::new("diffed_ceremony").unwrap(),
                CeremonyVersion::v1(),
                None,
                Vec::new(),
                Vec::new(),
                self.states,
                self.transitions,
                self.steps,
                self.guards,
                self.roles,
            )
            .unwrap()
        }
    }

    fn baseline() -> CeremonyDefinition {
        Draft::baseline().build()
    }

    #[test]
    fn a_definition_does_not_differ_from_itself() {
        let diff = CeremonyDefinitionDiff::between(&baseline(), &baseline());

        assert!(diff.is_identical());
        assert!(!diff.strands_running_sessions());
    }

    #[test]
    fn removing_a_state_strands_whoever_is_in_it() {
        let mut draft = Draft::baseline();
        draft.states = vec![
            CeremonyState::initial(state("OPEN")),
            CeremonyState::terminal(state("ELSEWHERE")),
        ];
        draft.transitions = vec![transition(
            "OPEN",
            "ELSEWHERE",
            "finish",
            vec![guard_name("work_done")],
        )];

        let diff = CeremonyDefinitionDiff::between(&baseline(), &draft.build());

        assert!(diff.strands_running_sessions());
        let removed = diff
            .changes()
            .iter()
            .find(|change| {
                change.kind() == CeremonyChangeKind::Removed
                    && change.locus() == &CeremonyValidationLocus::state(state("DONE"))
            })
            .expect("the state that went away should be reported");
        assert!(removed.impact().strands());
    }

    #[test]
    fn tightening_a_transition_can_block_a_session_and_relaxing_one_cannot() {
        let mut stricter = Draft::baseline();
        stricter.guards.push(CeremonyGuard::new(
            guard_name("human_approved"),
            GuardCondition::HumanApproval,
        ));
        stricter.transitions = vec![transition(
            "OPEN",
            "DONE",
            "finish",
            vec![guard_name("work_done"), guard_name("human_approved")],
        )];
        assert!(
            CeremonyDefinitionDiff::between(&baseline(), &stricter.build())
                .strands_running_sessions(),
            "a session about to move can be blocked by a guard that was not there"
        );

        let mut looser = Draft::baseline();
        looser.transitions = vec![transition("OPEN", "DONE", "finish", Vec::new())];
        looser.guards = Vec::new();
        assert!(
            !CeremonyDefinitionDiff::between(&baseline(), &looser.build())
                .strands_running_sessions(),
            "dropping a condition only ever lets a session through"
        );
    }

    #[test]
    fn narrowing_a_role_takes_something_away_and_widening_it_does_not() {
        let mut narrowed = Draft::baseline();
        narrowed.roles = vec![role(
            "FACILITATOR",
            BTreeSet::from([RoleAction::step(step_id("work"))]),
        )];
        assert!(
            CeremonyDefinitionDiff::between(&baseline(), &narrowed.build())
                .strands_running_sessions()
        );

        let mut widened = Draft::baseline();
        let mut actions = facilitator_actions();
        actions.insert(RoleAction::request_intervention());
        widened.roles = vec![role("FACILITATOR", actions)];
        assert!(
            !CeremonyDefinitionDiff::between(&baseline(), &widened.build())
                .strands_running_sessions()
        );
    }

    #[test]
    fn changing_how_the_work_is_done_leaves_a_session_where_it_was() {
        let mut draft = Draft::baseline();
        draft.steps = vec![step("work", "OPEN", "deliberation")];

        let diff = CeremonyDefinitionDiff::between(&baseline(), &draft.build());

        assert!(!diff.is_identical());
        assert!(
            !diff.strands_running_sessions(),
            "who does the work is not where the session is"
        );
        assert_eq!(diff.changes().len(), 1);
        assert_eq!(diff.changes()[0].detail(), "who does the work");
    }

    #[test]
    fn a_step_added_is_work_a_session_may_already_have_moved_past() {
        let mut draft = Draft::baseline();
        draft.steps.push(step("review", "OPEN", "noop"));
        let mut actions = facilitator_actions();
        actions.insert(RoleAction::step(step_id("review")));
        draft.roles = vec![role("FACILITATOR", actions)];

        let diff = CeremonyDefinitionDiff::between(&baseline(), &draft.build());

        // Counter-intuitive but the honest reading: a guard waiting on
        // every step to complete would never be satisfied for a session
        // already past the state this one sits in.
        assert!(diff.strands_running_sessions());
        assert_eq!(diff.strand_count(), 1);
    }
}
