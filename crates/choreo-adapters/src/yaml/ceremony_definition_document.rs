use std::collections::BTreeMap;

use choreo_core::entities::CeremonyDefinitionDraft;
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    CeremonyDescription, CeremonyName, CeremonyOutputDefinition, CeremonyVersion, OutputName,
};
use serde::Deserialize;
use serde_json::Value;

use super::ceremony_guard_document::CeremonyGuardDocument;
use super::ceremony_inputs_document::CeremonyInputsDocument;
use super::ceremony_role_document::CeremonyRoleDocument;
use super::ceremony_state_document::CeremonyStateDocument;
use super::ceremony_step_document::CeremonyStepDocument;
use super::ceremony_timeouts_document::CeremonyTimeoutsDocument;
use super::ceremony_transition_document::CeremonyTransitionDocument;
use super::retry_policies_document::RetryPoliciesDocument;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct CeremonyDefinitionDocument {
    version: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    inputs: CeremonyInputsDocument,
    #[serde(default)]
    outputs: BTreeMap<String, Value>,
    #[serde(default)]
    states: Vec<CeremonyStateDocument>,
    #[serde(default)]
    transitions: Vec<CeremonyTransitionDocument>,
    #[serde(default)]
    steps: Vec<CeremonyStepDocument>,
    #[serde(default)]
    guards: BTreeMap<String, CeremonyGuardDocument>,
    #[serde(default)]
    roles: Vec<CeremonyRoleDocument>,
    #[serde(default)]
    timeouts: CeremonyTimeoutsDocument,
    #[serde(default)]
    retry_policies: RetryPoliciesDocument,
}

impl CeremonyDefinitionDocument {
    /// Build the authoring draft without enforcing the state-machine
    /// invariants.
    ///
    /// Failures here are parse failures — a malformed identifier, an
    /// unusable retry policy — not structural defects. Structural
    /// defects belong in the draft's analysis report, where they can be
    /// reported all at once.
    pub(super) fn into_draft(self) -> Result<CeremonyDefinitionDraft, DomainError> {
        let retry_policy = self.retry_policies.default_policy()?;
        let timeout = self.timeouts.default_step_timeout()?;
        let steps = self
            .steps
            .into_iter()
            .map(|step| step.into_domain(retry_policy, timeout))
            .collect::<Result<Vec<_>, _>>()?;
        let step_ids = steps.iter().map(|step| step.id().clone()).collect();

        let transitions = self
            .transitions
            .into_iter()
            .map(CeremonyTransitionDocument::into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        let transition_triggers = transitions
            .iter()
            .map(|transition| transition.trigger().clone())
            .collect();

        let guards = self
            .guards
            .into_iter()
            .map(|(name, guard)| guard.into_domain(name))
            .collect::<Result<Vec<_>, _>>()?;
        let roles = self
            .roles
            .into_iter()
            .map(|role| role.into_domain(&step_ids, &transition_triggers))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CeremonyDefinitionDraft::new(
            CeremonyName::new(self.name)?,
            CeremonyVersion::new(self.version)?,
            self.description.map(CeremonyDescription::new).transpose()?,
            self.inputs.into_domain()?,
            self.outputs
                .into_keys()
                .map(|name| OutputName::new(name).map(CeremonyOutputDefinition::new))
                .collect::<Result<Vec<_>, _>>()?,
            self.states
                .into_iter()
                .map(CeremonyStateDocument::into_domain)
                .collect::<Result<Vec<_>, _>>()?,
            transitions,
            steps,
            guards,
            roles,
        ))
    }
}
