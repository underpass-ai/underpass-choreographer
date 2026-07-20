use std::collections::BTreeSet;

use choreo_core::error::DomainError;
use choreo_core::value_objects::{CeremonyRole, RoleAction, RoleId, StepId, TransitionTrigger};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct CeremonyRoleDocument {
    id: String,
    #[serde(default)]
    allowed_actions: Vec<String>,
}

impl CeremonyRoleDocument {
    pub(super) fn into_domain(
        self,
        step_ids: &BTreeSet<StepId>,
        transition_triggers: &BTreeSet<TransitionTrigger>,
    ) -> Result<CeremonyRole, DomainError> {
        CeremonyRole::new(
            RoleId::new(self.id)?,
            self.allowed_actions
                .into_iter()
                .map(|action| to_role_action(&action, step_ids, transition_triggers))
                .collect::<Result<Vec<_>, _>>()?,
        )
    }
}

fn to_role_action(
    raw: &str,
    step_ids: &BTreeSet<StepId>,
    transition_triggers: &BTreeSet<TransitionTrigger>,
) -> Result<RoleAction, DomainError> {
    let matches_step = step_ids.iter().any(|step_id| step_id.as_str() == raw);
    let matches_transition = transition_triggers
        .iter()
        .any(|trigger| trigger.as_str() == raw);
    let capability = RoleAction::from_capability_label(raw);

    match (matches_step, matches_transition, capability) {
        (true, false, None) => Ok(RoleAction::step(StepId::new(raw)?)),
        (false, true, None) => Ok(RoleAction::transition(TransitionTrigger::new(raw)?)),
        (false, false, Some(capability)) => Ok(capability),
        (true, true, _) | (true, false, Some(_)) | (false, true, Some(_)) => {
            Err(DomainError::InvariantViolated {
                reason: "ceremony role action is ambiguous",
            })
        }
        (false, false, None) => speculative_role_action(raw),
    }
}

fn speculative_role_action(raw: &str) -> Result<RoleAction, DomainError> {
    if let Ok(step_id) = StepId::new(raw) {
        Ok(RoleAction::step(step_id))
    } else {
        TransitionTrigger::new(raw).map(RoleAction::transition)
    }
}
