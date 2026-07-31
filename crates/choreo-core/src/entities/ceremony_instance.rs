//! [`CeremonyInstance`] aggregate.
//!
//! Runtime state for a single ceremony execution. The aggregate owns
//! step leases, retry attempts, idempotency keys and state transitions,
//! so failover remains a domain rule instead of adapter glue.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{
    ceremony_definition::CeremonyDefinition, CeremonyIntervention, PublishedCeremonyDefinition,
};
use crate::error::DomainError;
use crate::ports::CeremonyEvidenceRequest;
use crate::value_objects::{
    CeremonyContext, CeremonyDefinitionDigest, CeremonyEvidenceSourceId, CeremonyGuardDeferral,
    CeremonyGuardDeferralContent, CeremonyId, CeremonyInterventionContent, CeremonyInterventionId,
    CeremonyInterventionKind, CeremonyInterventionProvenance, CeremonyInterventionTarget,
    CeremonyName, CeremonyParticipantBinding, CeremonyVersion, GuardCondition, GuardName,
    IdempotencyKey, RoleAction, RoleId, Specialty, StateId, StepAttempt, StepExecutionRecord,
    StepId, StepLease, StepResult, StepStatus, TransitionTrigger,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CeremonyInstance {
    id: CeremonyId,
    definition_name: CeremonyName,
    definition_version: CeremonyVersion,
    current_state: StateId,
    step_records: BTreeMap<StepId, StepExecutionRecord>,
    #[serde(default)]
    interventions: Vec<CeremonyIntervention>,
    #[serde(default)]
    guard_deferrals: Vec<CeremonyGuardDeferral>,
    /// Who sits in each seat for this session, where anyone was
    /// seated. A role with no binding is played the way the definition
    /// says, which is the usual case and not a lesser one.
    #[serde(default)]
    participant_bindings: BTreeMap<RoleId, CeremonyParticipantBinding>,
    context: CeremonyContext,
    idempotency_keys: BTreeSet<IdempotencyKey>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    completed_at: Option<OffsetDateTime>,
    /// The published definition this instance is bound to, when it was
    /// started from one.
    ///
    /// Absent for an instance started from a definition handed in at
    /// the time — which is a real and useful way to work, and not the
    /// same thing. Recording which of the two happened is the point: a
    /// name and a version identify a published definition only while
    /// publication is immutable, and an instance that also carries the
    /// digest can be checked against the definition rather than trusted
    /// to have run it.
    #[serde(default)]
    bound_definition: Option<CeremonyDefinitionDigest>,
}

impl CeremonyInstance {
    /// Start from a definition supplied for this run.
    ///
    /// Nothing binds the instance to a definition that can be looked up
    /// later; that is what [`Self::start_bound`] is for.
    #[must_use]
    pub fn start(
        id: CeremonyId,
        definition: &CeremonyDefinition,
        context: CeremonyContext,
        now: OffsetDateTime,
    ) -> Self {
        Self::open(id, definition, context, now, None)
    }

    /// Start from a published definition, recording its digest.
    ///
    /// The digest travels with the instance so a later reader can
    /// verify which definition ran instead of taking the name and
    /// version on trust.
    #[must_use]
    pub fn start_bound(
        id: CeremonyId,
        published: &PublishedCeremonyDefinition,
        context: CeremonyContext,
        now: OffsetDateTime,
    ) -> Self {
        Self::open(
            id,
            published.definition(),
            context,
            now,
            Some(published.digest()),
        )
    }

    fn open(
        id: CeremonyId,
        definition: &CeremonyDefinition,
        context: CeremonyContext,
        now: OffsetDateTime,
        bound_definition: Option<CeremonyDefinitionDigest>,
    ) -> Self {
        let step_records = definition
            .steps()
            .keys()
            .map(|step_id| (step_id.clone(), StepExecutionRecord::pending()))
            .collect();

        Self {
            id,
            definition_name: definition.name().clone(),
            definition_version: definition.version().clone(),
            current_state: definition.initial_state_id().clone(),
            step_records,
            interventions: Vec::new(),
            guard_deferrals: Vec::new(),
            participant_bindings: BTreeMap::new(),
            context,
            idempotency_keys: BTreeSet::new(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            bound_definition,
        }
    }

    /// The digest of the published definition this instance runs, if it
    /// was started from one.
    #[must_use]
    pub fn bound_definition(&self) -> Option<CeremonyDefinitionDigest> {
        self.bound_definition
    }

    /// Whether this instance runs a definition that can be looked up
    /// and checked, rather than one supplied for the run.
    #[must_use]
    pub fn is_bound_to_a_published_definition(&self) -> bool {
        self.bound_definition.is_some()
    }

    #[must_use]
    pub fn id(&self) -> &CeremonyId {
        &self.id
    }

    #[must_use]
    pub fn definition_name(&self) -> &CeremonyName {
        &self.definition_name
    }

    #[must_use]
    pub fn definition_version(&self) -> &CeremonyVersion {
        &self.definition_version
    }

    #[must_use]
    pub fn current_state(&self) -> &StateId {
        &self.current_state
    }

    #[must_use]
    pub fn step_records(&self) -> &BTreeMap<StepId, StepExecutionRecord> {
        &self.step_records
    }

    #[must_use]
    pub fn step_record(&self, step_id: &StepId) -> Option<&StepExecutionRecord> {
        self.step_records.get(step_id)
    }

    #[must_use]
    pub fn interventions(&self) -> &[CeremonyIntervention] {
        &self.interventions
    }

    #[must_use]
    pub fn guard_deferrals(&self) -> &[CeremonyGuardDeferral] {
        &self.guard_deferrals
    }

    #[must_use]
    pub fn intervention(
        &self,
        intervention_id: &CeremonyInterventionId,
    ) -> Option<&CeremonyIntervention> {
        self.interventions
            .iter()
            .find(|intervention| intervention.id() == intervention_id)
    }

    #[must_use]
    pub fn context(&self) -> &CeremonyContext {
        &self.context
    }

    #[must_use]
    pub fn idempotency_keys(&self) -> &BTreeSet<IdempotencyKey> {
        &self.idempotency_keys
    }

    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    #[must_use]
    pub fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }

    #[must_use]
    pub fn completed_at(&self) -> Option<OffsetDateTime> {
        self.completed_at
    }

    #[must_use]
    pub fn is_terminal(&self, definition: &CeremonyDefinition) -> bool {
        self.matches_definition(definition) && definition.is_terminal_state(&self.current_state)
    }

    #[must_use]
    pub fn is_completed(&self, definition: &CeremonyDefinition) -> bool {
        self.is_terminal(definition) && self.completed_at.is_some()
    }

    pub fn start_step_as(
        &mut self,
        definition: &CeremonyDefinition,
        role_id: &RoleId,
        step_id: &StepId,
        lease: StepLease,
        now: OffsetDateTime,
    ) -> Result<StepAttempt, DomainError> {
        self.require_role(definition, role_id, &RoleAction::step(step_id.clone()))?;
        self.start_step(definition, step_id, lease, now)
    }

    pub fn start_step(
        &mut self,
        definition: &CeremonyDefinition,
        step_id: &StepId,
        lease: StepLease,
        now: OffsetDateTime,
    ) -> Result<StepAttempt, DomainError> {
        self.require_definition(definition)?;
        if self.is_terminal(definition) {
            return Err(DomainError::InvariantViolated {
                reason: "terminal ceremony instances cannot start steps",
            });
        }

        let step = definition.step(step_id).ok_or(DomainError::NotFound {
            what: "ceremony_instance.step",
        })?;
        if step.state_id() != &self.current_state {
            return Err(DomainError::InvalidTransition {
                from: "ceremony_instance.current_state",
                to: "ceremony_step.state",
            });
        }

        let record = self
            .step_records
            .get(step_id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_instance.step_record",
            })?;
        if !record.can_be_started_at(now) {
            return Err(DomainError::InvariantViolated {
                reason: "step lease is still active",
            });
        }

        let next_attempt = next_attempt_for_start(&record)?;
        if !step.retry_policy().allows_attempt(next_attempt) {
            return Err(DomainError::InvariantViolated {
                reason: "step retry policy exhausted",
            });
        }
        if !self
            .idempotency_keys
            .insert(lease.idempotency_key().clone())
        {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_instance.idempotency_key",
            });
        }

        self.step_records
            .insert(step_id.clone(), record.with_started(lease, next_attempt));
        self.updated_at = now;
        Ok(next_attempt)
    }

    pub fn apply_step_result(
        &mut self,
        definition: &CeremonyDefinition,
        step_id: &StepId,
        result: StepResult,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_definition(definition)?;
        let step = definition.step(step_id).ok_or(DomainError::NotFound {
            what: "ceremony_instance.step",
        })?;
        if step.state_id() != &self.current_state {
            return Err(DomainError::InvalidTransition {
                from: "ceremony_instance.current_state",
                to: "ceremony_step.state",
            });
        }

        let record = self
            .step_records
            .get(step_id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_instance.step_record",
            })?;
        if record.status() != StepStatus::InProgress {
            return Err(DomainError::InvariantViolated {
                reason: "step result requires an in-progress step",
            });
        }

        self.step_records
            .insert(step_id.clone(), record.with_result(result));
        self.updated_at = now;
        Ok(())
    }

    /// Approving is checked the way deferring is. It used to take no
    /// definition at all, so any name at all could be "approved" —
    /// which wrote that name into the session context, told the caller
    /// it had succeeded, and left a session that would never move.
    ///
    /// Approving ahead of time is still allowed: unlike a deferral,
    /// which answers a decision being asked for now, a person may
    /// settle a guard before the work leading up to it is finished.
    pub fn approve_guard(
        &mut self,
        definition: &CeremonyDefinition,
        guard_name: &GuardName,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot approve guards",
        )?;
        let guard = definition
            .guards()
            .get(guard_name)
            .ok_or(DomainError::NotFound {
                what: "ceremony_guard",
            })?;
        if !matches!(guard.condition(), GuardCondition::HumanApproval) {
            return Err(DomainError::InvariantViolated {
                reason: "only human approval guards can be approved",
            });
        }
        self.context = self.context.clone().with_guard_approval(guard_name)?;
        self.updated_at = now;
        Ok(())
    }

    /// Seat a role for this session.
    ///
    /// Rebinding is allowed and deliberate: a panel can become
    /// unavailable halfway through a working session, and a ceremony
    /// that could not be re-seated would have to be abandoned and
    /// started again, losing everything already decided. What was
    /// seated before stays in the journal; the instance carries who is
    /// seated now, which is what the next step needs.
    pub fn bind_participant(
        &mut self,
        definition: &CeremonyDefinition,
        role_id: RoleId,
        specialty: Specialty,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot be re-seated",
        )?;
        // A seat that the ceremony never declared is not a seat.
        if definition.role(&role_id).is_none() {
            return Err(DomainError::NotFound {
                what: "ceremony_role",
            });
        }
        self.participant_bindings.insert(
            role_id.clone(),
            CeremonyParticipantBinding::record(role_id, specialty, now),
        );
        self.updated_at = now;
        Ok(())
    }

    #[must_use]
    pub fn participant_bindings(&self) -> &BTreeMap<RoleId, CeremonyParticipantBinding> {
        &self.participant_bindings
    }

    /// The specialty a role's work should be put to, if this session
    /// seated one. `None` means the definition decides, as usual.
    #[must_use]
    pub fn bound_specialty(&self, role_id: &RoleId) -> Option<&Specialty> {
        self.participant_bindings
            .get(role_id)
            .map(CeremonyParticipantBinding::specialty)
    }

    pub fn defer_guard(
        &mut self,
        definition: &CeremonyDefinition,
        guard_name: GuardName,
        content: CeremonyGuardDeferralContent,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot defer guard decisions",
        )?;
        let guard = definition
            .guards()
            .get(&guard_name)
            .ok_or(DomainError::NotFound {
                what: "ceremony_guard",
            })?;
        if !matches!(guard.condition(), GuardCondition::HumanApproval) {
            return Err(DomainError::InvariantViolated {
                reason: "only human approval guards can be deferred",
            });
        }
        if self.context.is_guard_approved(&guard_name) {
            return Err(DomainError::InvariantViolated {
                reason: "approved human guards cannot be deferred",
            });
        }
        let is_currently_required = definition
            .available_transitions(&self.current_state)
            .any(|transition| transition.required_guards().contains(&guard_name));
        if !is_currently_required {
            return Err(DomainError::InvariantViolated {
                reason: "human guard is not required from the current state",
            });
        }

        self.guard_deferrals
            .push(CeremonyGuardDeferral::record(guard_name, content, now));
        self.updated_at = now;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn request_intervention_as(
        &mut self,
        definition: &CeremonyDefinition,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        kind: CeremonyInterventionKind,
        target: CeremonyInterventionTarget,
        content: CeremonyInterventionContent,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.request_intervention_with_provenance_as(
            definition,
            intervention_id,
            role_id,
            kind,
            target,
            content,
            None,
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn request_intervention_with_provenance_as(
        &mut self,
        definition: &CeremonyDefinition,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        kind: CeremonyInterventionKind,
        target: CeremonyInterventionTarget,
        content: CeremonyInterventionContent,
        provenance: Option<CeremonyInterventionProvenance>,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot accept interventions",
        )?;
        self.require_role(definition, &role_id, &RoleAction::request_intervention())?;
        Self::require_intervention_target(definition, &target)?;
        if let Some(provenance) = provenance.as_ref() {
            self.require_intervention_provenance(definition, &role_id, &target, provenance)?;
        }
        if self
            .interventions
            .iter()
            .any(|intervention| intervention.id() == &intervention_id)
        {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_intervention",
            });
        }
        let intervention = CeremonyIntervention::open_with_provenance(
            intervention_id,
            kind,
            role_id,
            target,
            content,
            provenance,
            now,
        );
        self.interventions.push(intervention);
        self.updated_at = now;
        Ok(())
    }

    pub fn respond_to_intervention_as(
        &mut self,
        definition: &CeremonyDefinition,
        intervention_id: &CeremonyInterventionId,
        role_id: RoleId,
        content: CeremonyInterventionContent,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot receive intervention responses",
        )?;
        self.require_role(definition, &role_id, &RoleAction::respond_to_intervention())?;
        self.interventions
            .iter_mut()
            .find(|intervention| intervention.id() == intervention_id)
            .ok_or(DomainError::NotFound {
                what: "ceremony_intervention",
            })?
            .respond(role_id, content, now)?;
        self.updated_at = now;
        Ok(())
    }

    pub fn prepare_evidence_request_as(
        &self,
        definition: &CeremonyDefinition,
        intervention_id: CeremonyInterventionId,
        role_id: RoleId,
        source_id: CeremonyEvidenceSourceId,
        query: CeremonyInterventionContent,
    ) -> Result<CeremonyEvidenceRequest, DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot collect intervention evidence",
        )?;
        self.require_role(definition, &role_id, &RoleAction::respond_to_intervention())?;
        self.intervention(&intervention_id)
            .ok_or(DomainError::NotFound {
                what: "ceremony_intervention",
            })?
            .ensure_can_respond(&role_id)?;
        Ok(CeremonyEvidenceRequest::new(
            self.id.clone(),
            intervention_id,
            role_id,
            source_id,
            query,
            self.context.clone(),
        ))
    }

    pub fn respond_to_intervention_with_evidence_as(
        &mut self,
        definition: &CeremonyDefinition,
        intervention_id: &CeremonyInterventionId,
        role_id: RoleId,
        evidence_pack: super::CeremonyEvidencePack,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot receive intervention evidence",
        )?;
        self.require_role(definition, &role_id, &RoleAction::respond_to_intervention())?;
        self.interventions
            .iter_mut()
            .find(|intervention| intervention.id() == intervention_id)
            .ok_or(DomainError::NotFound {
                what: "ceremony_intervention",
            })?
            .respond_with_evidence(role_id, evidence_pack, now)?;
        self.updated_at = now;
        Ok(())
    }

    pub fn close_intervention_as(
        &mut self,
        definition: &CeremonyDefinition,
        intervention_id: &CeremonyInterventionId,
        role_id: &RoleId,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_active(
            definition,
            "terminal ceremony instances cannot close interventions",
        )?;
        self.require_role(definition, role_id, &RoleAction::request_intervention())?;
        self.interventions
            .iter_mut()
            .find(|intervention| intervention.id() == intervention_id)
            .ok_or(DomainError::NotFound {
                what: "ceremony_intervention",
            })?
            .close(role_id, now)?;
        self.updated_at = now;
        Ok(())
    }

    pub fn apply_transition_as(
        &mut self,
        definition: &CeremonyDefinition,
        role_id: &RoleId,
        trigger: &TransitionTrigger,
        now: OffsetDateTime,
    ) -> Result<StateId, DomainError> {
        self.require_role(
            definition,
            role_id,
            &RoleAction::transition(trigger.clone()),
        )?;
        self.apply_transition(definition, trigger, now)
    }

    pub fn apply_transition(
        &mut self,
        definition: &CeremonyDefinition,
        trigger: &TransitionTrigger,
        now: OffsetDateTime,
    ) -> Result<StateId, DomainError> {
        self.require_definition(definition)?;
        if self.is_terminal(definition) {
            return Err(DomainError::InvariantViolated {
                reason: "terminal ceremony instances cannot transition",
            });
        }

        let transition = definition
            .transition_for_trigger(&self.current_state, trigger)
            .ok_or(DomainError::InvalidTransition {
                from: "ceremony_instance.current_state",
                to: "transition_trigger",
            })?;
        if !definition.guards_are_satisfied(transition, &self.step_records, &self.context) {
            return Err(DomainError::InvariantViolated {
                reason: "ceremony transition guards are not satisfied",
            });
        }

        self.current_state = transition.to().clone();
        self.updated_at = now;
        if definition.is_terminal_state(&self.current_state) {
            self.completed_at = Some(now);
        }
        Ok(self.current_state.clone())
    }

    fn matches_definition(&self, definition: &CeremonyDefinition) -> bool {
        self.definition_name == *definition.name()
            && self.definition_version == *definition.version()
    }

    fn require_definition(&self, definition: &CeremonyDefinition) -> Result<(), DomainError> {
        if self.matches_definition(definition) {
            Ok(())
        } else {
            Err(DomainError::InvariantViolated {
                reason: "ceremony instance definition mismatch",
            })
        }
    }

    fn require_active(
        &self,
        definition: &CeremonyDefinition,
        terminal_reason: &'static str,
    ) -> Result<(), DomainError> {
        self.require_definition(definition)?;
        if self.is_terminal(definition) {
            Err(DomainError::InvariantViolated {
                reason: terminal_reason,
            })
        } else {
            Ok(())
        }
    }

    fn require_intervention_target(
        definition: &CeremonyDefinition,
        target: &CeremonyInterventionTarget,
    ) -> Result<(), DomainError> {
        let Some(role_ids) = target.role_ids() else {
            return Ok(());
        };
        for role_id in role_ids {
            if definition.role(role_id).is_none() {
                return Err(DomainError::NotFound {
                    what: "ceremony_intervention.target_role",
                });
            }
            if !definition.role_allows(role_id, &RoleAction::respond_to_intervention()) {
                return Err(DomainError::InvariantViolated {
                    reason: "target role cannot respond to ceremony interventions",
                });
            }
        }
        Ok(())
    }

    fn require_intervention_provenance(
        &self,
        definition: &CeremonyDefinition,
        requested_by: &RoleId,
        target: &CeremonyInterventionTarget,
        provenance: &CeremonyInterventionProvenance,
    ) -> Result<(), DomainError> {
        let source = self
            .intervention(provenance.source_intervention_id())
            .ok_or(DomainError::NotFound {
                what: "ceremony_intervention.provenance_source",
            })?;
        if source.requested_by() != requested_by {
            return Err(DomainError::InvariantViolated {
                reason: "only the source requester can select an intervention response",
            });
        }
        if !source
            .responses()
            .iter()
            .any(|response| response.role_id() == provenance.source_response_role_id())
        {
            return Err(DomainError::NotFound {
                what: "ceremony_intervention.provenance_response",
            });
        }
        if definition.role(provenance.selected_role_id()).is_none() {
            return Err(DomainError::NotFound {
                what: "ceremony_intervention.provenance_selected_role",
            });
        }
        if !definition.role_allows(
            provenance.selected_role_id(),
            &RoleAction::respond_to_intervention(),
        ) {
            return Err(DomainError::InvariantViolated {
                reason: "selected intervention role cannot respond",
            });
        }
        if !target.accepts(provenance.selected_role_id()) {
            return Err(DomainError::InvariantViolated {
                reason: "intervention target does not include the selected role",
            });
        }
        Ok(())
    }

    fn require_role(
        &self,
        definition: &CeremonyDefinition,
        role_id: &RoleId,
        action: &RoleAction,
    ) -> Result<(), DomainError> {
        self.require_definition(definition)?;
        if definition.role_allows(role_id, action) {
            Ok(())
        } else {
            Err(DomainError::InvariantViolated {
                reason: "ceremony role is not allowed to perform action",
            })
        }
    }
}

fn next_attempt_for_start(record: &StepExecutionRecord) -> Result<StepAttempt, DomainError> {
    if matches!(record.status(), StepStatus::Failed | StepStatus::InProgress) {
        record.attempt().next()
    } else {
        Ok(record.attempt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::{
        Attributes, CeremonyGuard, CeremonyState, CeremonyStep, CeremonyTransition, GuardCondition,
        GuardName, LeaseOwnerId, RetryPolicy, StepHandlerConfig, StepHandlerKind, StepOutput,
    };
    use time::macros::datetime;

    fn now() -> OffsetDateTime {
        datetime!(2026-06-06 12:00:00 UTC)
    }

    fn state_id(raw: &str) -> StateId {
        StateId::new(raw).unwrap()
    }

    fn step_id(raw: &str) -> StepId {
        StepId::new(raw).unwrap()
    }

    fn trigger(raw: &str) -> TransitionTrigger {
        TransitionTrigger::new(raw).unwrap()
    }

    fn role_id(raw: &str) -> RoleId {
        RoleId::new(raw).unwrap()
    }

    fn guard_name(raw: &str) -> GuardName {
        GuardName::new(raw).unwrap()
    }

    fn handler_kind() -> StepHandlerKind {
        StepHandlerKind::new("multiagent_round").unwrap()
    }

    fn retrying_step(raw_step_id: &str, raw_state_id: &str) -> CeremonyStep {
        CeremonyStep::new(
            step_id(raw_step_id),
            state_id(raw_state_id),
            handler_kind(),
            StepHandlerConfig::empty(),
            RetryPolicy::new(
                StepAttempt::new(3).unwrap(),
                crate::value_objects::DurationMs::ZERO,
            ),
            None,
        )
    }

    fn single_attempt_step(raw_step_id: &str, raw_state_id: &str) -> CeremonyStep {
        CeremonyStep::new(
            step_id(raw_step_id),
            state_id(raw_state_id),
            handler_kind(),
            StepHandlerConfig::empty(),
            RetryPolicy::single_attempt(),
            None,
        )
    }

    fn lease(
        raw_owner_id: &str,
        raw_key: &str,
        acquired_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> StepLease {
        StepLease::new(
            LeaseOwnerId::new(raw_owner_id).unwrap(),
            IdempotencyKey::new(raw_key).unwrap(),
            acquired_at,
            expires_at,
        )
        .unwrap()
    }

    fn role(actions: Vec<RoleAction>) -> crate::value_objects::CeremonyRole {
        crate::value_objects::CeremonyRole::new(role_id("facilitator"), actions).unwrap()
    }

    fn definition_with_steps(steps: Vec<CeremonyStep>) -> CeremonyDefinition {
        let plan_done = CeremonyGuard::new(
            guard_name("plan_done"),
            GuardCondition::StepStatus {
                step_id: step_id("plan"),
                status: StepStatus::Completed,
            },
        );
        let finish = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("finish"),
            vec![plan_done.name().clone()],
        )
        .unwrap();
        let role = role(vec![
            RoleAction::step(step_id("plan")),
            RoleAction::transition(finish.trigger().clone()),
            RoleAction::request_intervention(),
        ]);
        let observer = crate::value_objects::CeremonyRole::new(
            role_id("observer"),
            vec![RoleAction::respond_to_intervention()],
        )
        .unwrap();

        CeremonyDefinition::new(
            crate::value_objects::CeremonyName::new("planning_ceremony").unwrap(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::intermediate(state_id("review")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![finish],
            steps,
            vec![plan_done],
            vec![role, observer],
        )
        .unwrap()
    }

    fn definition() -> CeremonyDefinition {
        definition_with_steps(vec![
            retrying_step("plan", "drafting"),
            single_attempt_step("review_step", "review"),
        ])
    }

    fn instance(definition: &CeremonyDefinition) -> CeremonyInstance {
        CeremonyInstance::start(
            CeremonyId::new("ceremony-1").unwrap(),
            definition,
            CeremonyContext::empty(),
            now(),
        )
    }

    #[test]
    fn starts_in_initial_state_with_pending_records() {
        let definition = definition();
        let instance = instance(&definition);

        assert_eq!(instance.current_state(), &state_id("drafting"));
        assert_eq!(
            instance.step_record(&step_id("plan")).unwrap().status(),
            StepStatus::Pending
        );
        assert_eq!(
            instance
                .step_record(&step_id("review_step"))
                .unwrap()
                .status(),
            StepStatus::Pending
        );
    }

    #[test]
    fn dynamic_intervention_collects_role_scoped_response_and_requester_closes_it() {
        let definition = definition();
        let mut instance = instance(&definition);
        let intervention_id = CeremonyInterventionId::new("queue-check").unwrap();
        let facilitator = role_id("facilitator");
        let observer = role_id("observer");

        instance
            .request_intervention_as(
                &definition,
                intervention_id.clone(),
                facilitator.clone(),
                CeremonyInterventionKind::Investigation,
                CeremonyInterventionTarget::roles([observer.clone()]).unwrap(),
                CeremonyInterventionContent::new(
                    "Inspect the queue without consuming messages.",
                    Attributes::empty(),
                )
                .unwrap(),
                now(),
            )
            .unwrap();
        instance
            .respond_to_intervention_as(
                &definition,
                &intervention_id,
                observer.clone(),
                CeremonyInterventionContent::new("Queue depth is stable.", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap();
        let selected_intervention_id = CeremonyInterventionId::new("selected-check").unwrap();
        instance
            .request_intervention_with_provenance_as(
                &definition,
                selected_intervention_id.clone(),
                facilitator.clone(),
                CeremonyInterventionKind::Investigation,
                CeremonyInterventionTarget::roles([observer.clone()]).unwrap(),
                CeremonyInterventionContent::new(
                    "Inspect the proposed signal.",
                    Attributes::empty(),
                )
                .unwrap(),
                Some(CeremonyInterventionProvenance::selected_from(
                    intervention_id.clone(),
                    observer.clone(),
                    observer.clone(),
                )),
                now(),
            )
            .unwrap();
        instance
            .close_intervention_as(&definition, &intervention_id, &facilitator, now())
            .unwrap();

        let intervention = instance.intervention(&intervention_id).unwrap();
        assert_eq!(intervention.responses().len(), 1);
        assert_eq!(
            intervention.status(),
            crate::value_objects::CeremonyInterventionStatus::Closed
        );
        let provenance = instance
            .intervention(&selected_intervention_id)
            .unwrap()
            .provenance()
            .unwrap();
        assert_eq!(provenance.source_intervention_id(), &intervention_id);
        assert_eq!(provenance.selected_role_id(), &observer);
    }

    #[test]
    fn intervention_rejects_roles_without_the_required_capability() {
        let definition = definition();
        let mut instance = instance(&definition);

        let error = instance
            .request_intervention_as(
                &definition,
                CeremonyInterventionId::new("not-allowed").unwrap(),
                role_id("observer"),
                CeremonyInterventionKind::Opinion,
                CeremonyInterventionTarget::table(),
                CeremonyInterventionContent::new("What do you think?", Attributes::empty())
                    .unwrap(),
                now(),
            )
            .unwrap_err();

        assert!(matches!(error, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn rejects_step_execution_outside_current_state() {
        let definition = definition();
        let mut instance = instance(&definition);

        let err = instance
            .start_step(
                &definition,
                &step_id("review_step"),
                lease(
                    "runner-1",
                    "key-1",
                    now(),
                    datetime!(2026-06-06 12:05:00 UTC),
                ),
                now(),
            )
            .unwrap_err();

        assert!(matches!(err, DomainError::InvalidTransition { .. }));
    }

    #[test]
    fn completed_step_unlocks_guarded_transition() {
        let definition = definition();
        let mut instance = instance(&definition);

        instance
            .start_step_as(
                &definition,
                &role_id("facilitator"),
                &step_id("plan"),
                lease(
                    "runner-1",
                    "key-1",
                    now(),
                    datetime!(2026-06-06 12:05:00 UTC),
                ),
                now(),
            )
            .unwrap();
        instance
            .apply_step_result(
                &definition,
                &step_id("plan"),
                StepResult::completed(StepOutput::empty()).unwrap(),
                datetime!(2026-06-06 12:01:00 UTC),
            )
            .unwrap();
        let state = instance
            .apply_transition_as(
                &definition,
                &role_id("facilitator"),
                &trigger("finish"),
                datetime!(2026-06-06 12:02:00 UTC),
            )
            .unwrap();

        assert_eq!(state, state_id("done"));
        assert!(instance.is_completed(&definition));
    }

    #[test]
    fn active_lease_blocks_failover_takeover() {
        let definition = definition();
        let mut instance = instance(&definition);

        instance
            .start_step(
                &definition,
                &step_id("plan"),
                lease(
                    "runner-1",
                    "key-1",
                    now(),
                    datetime!(2026-06-06 12:05:00 UTC),
                ),
                now(),
            )
            .unwrap();
        let err = instance
            .start_step(
                &definition,
                &step_id("plan"),
                lease(
                    "runner-2",
                    "key-2",
                    datetime!(2026-06-06 12:01:00 UTC),
                    datetime!(2026-06-06 12:06:00 UTC),
                ),
                datetime!(2026-06-06 12:01:00 UTC),
            )
            .unwrap_err();

        assert!(matches!(err, DomainError::InvariantViolated { .. }));
        assert_eq!(
            instance
                .step_record(&step_id("plan"))
                .unwrap()
                .lease()
                .unwrap()
                .owner_id()
                .as_str(),
            "runner-1"
        );
    }

    #[test]
    fn expired_lease_allows_failover_takeover_with_next_attempt() {
        let definition = definition();
        let mut instance = instance(&definition);

        instance
            .start_step(
                &definition,
                &step_id("plan"),
                lease(
                    "runner-1",
                    "key-1",
                    now(),
                    datetime!(2026-06-06 12:05:00 UTC),
                ),
                now(),
            )
            .unwrap();
        let attempt = instance
            .start_step(
                &definition,
                &step_id("plan"),
                lease(
                    "runner-2",
                    "key-2",
                    datetime!(2026-06-06 12:06:00 UTC),
                    datetime!(2026-06-06 12:11:00 UTC),
                ),
                datetime!(2026-06-06 12:06:00 UTC),
            )
            .unwrap();

        assert_eq!(attempt, StepAttempt::new(2).unwrap());
        let record = instance.step_record(&step_id("plan")).unwrap();
        assert_eq!(record.attempt(), StepAttempt::new(2).unwrap());
        assert_eq!(record.lease().unwrap().owner_id().as_str(), "runner-2");
    }

    #[test]
    fn approving_a_guard_the_ceremony_never_declared_is_refused() {
        let approval =
            CeremonyGuard::new(guard_name("human_approved"), GuardCondition::HumanApproval);
        let finish = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("approve"),
            vec![approval.name().clone()],
        )
        .unwrap();
        let definition = CeremonyDefinition::new(
            crate::value_objects::CeremonyName::new("approval_ceremony").unwrap(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![finish.clone()],
            Vec::new(),
            vec![approval],
            vec![role(vec![RoleAction::transition(finish.trigger().clone())])],
        )
        .unwrap();
        let mut instance = instance(&definition);

        // This used to succeed and write `not_a_guard: true` into the
        // session context: a caller could put any key at all there,
        // and a typo answered "approved" while leaving a session that
        // would never move.
        assert!(matches!(
            instance.approve_guard(&definition, &guard_name("not_a_guard"), now()),
            Err(DomainError::NotFound {
                what: "ceremony_guard"
            })
        ));
        assert!(!instance
            .context()
            .is_guard_approved(&guard_name("not_a_guard")));
    }

    #[test]
    fn human_approval_guard_uses_typed_context() {
        let approval =
            CeremonyGuard::new(guard_name("human_approved"), GuardCondition::HumanApproval);
        let finish = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("approve"),
            vec![approval.name().clone()],
        )
        .unwrap();
        let definition = CeremonyDefinition::new(
            crate::value_objects::CeremonyName::new("approval_ceremony").unwrap(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![finish.clone()],
            Vec::new(),
            vec![approval.clone()],
            vec![role(vec![RoleAction::transition(finish.trigger().clone())])],
        )
        .unwrap();
        let mut instance = instance(&definition);

        assert!(matches!(
            instance.apply_transition(&definition, &trigger("approve"), now()),
            Err(DomainError::InvariantViolated { .. })
        ));
        instance
            .approve_guard(
                &definition,
                approval.name(),
                datetime!(2026-06-06 12:01:00 UTC),
            )
            .unwrap();
        instance
            .apply_transition(
                &definition,
                &trigger("approve"),
                datetime!(2026-06-06 12:02:00 UTC),
            )
            .unwrap();

        assert!(instance.is_completed(&definition));
    }

    #[test]
    fn human_guard_deferral_preserves_uncertainty_without_approving() {
        let approval =
            CeremonyGuard::new(guard_name("human_approved"), GuardCondition::HumanApproval);
        let finish = CeremonyTransition::new(
            state_id("drafting"),
            state_id("done"),
            trigger("approve"),
            vec![approval.name().clone()],
        )
        .unwrap();
        let definition = CeremonyDefinition::new(
            crate::value_objects::CeremonyName::new("deferral_ceremony").unwrap(),
            CeremonyVersion::v1(),
            None,
            Vec::new(),
            Vec::new(),
            vec![
                CeremonyState::initial(state_id("drafting")),
                CeremonyState::terminal(state_id("done")),
            ],
            vec![finish.clone()],
            Vec::new(),
            vec![approval.clone()],
            vec![role(vec![RoleAction::transition(finish.trigger().clone())])],
        )
        .unwrap();
        let mut instance = instance(&definition);

        instance
            .defer_guard(
                &definition,
                approval.name().clone(),
                CeremonyGuardDeferralContent::new(
                    "I do not know.",
                    "I cannot explain how the issue was resolved.",
                    vec!["New evidence explains the resolution.".to_owned()],
                )
                .unwrap(),
                datetime!(2026-06-06 12:01:00 UTC),
            )
            .unwrap();

        assert!(!instance.context().is_guard_approved(approval.name()));
        assert!(instance
            .apply_transition(&definition, &trigger("approve"), now())
            .is_err());
        let deferral = &instance.guard_deferrals()[0];
        assert_eq!(deferral.guard_name(), approval.name());
        assert_eq!(deferral.content().statement(), "I do not know.");
    }
}
