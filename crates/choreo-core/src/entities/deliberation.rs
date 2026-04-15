//! [`Deliberation`] aggregate.
//!
//! The Deliberation is the central aggregate root of the Choreographer.
//! It owns the lifecycle of one deliberation from proposal generation
//! through peer review to scoring and completion. State transitions
//! are explicit and protected so no caller can place the aggregate in
//! an inconsistent shape.
//!
//! Phase graph (linear):
//!
//! ```text
//! Proposing -> Critiquing -> Revising -> Validating -> Scoring -> Completed
//! ```
//!
//! Transitions are one-way. Methods reject operations that do not
//! match the current phase.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::entities::proposal::Proposal;
use crate::entities::validation::ValidationOutcome;
use crate::error::DomainError;
use crate::value_objects::{DurationMs, ProposalId, Rounds, Specialty, TaskId};

/// Lifecycle phases of a deliberation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeliberationPhase {
    Proposing,
    Critiquing,
    Revising,
    Validating,
    Scoring,
    Completed,
}

impl DeliberationPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Proposing => "Proposing",
            Self::Critiquing => "Critiquing",
            Self::Revising => "Revising",
            Self::Validating => "Validating",
            Self::Scoring => "Scoring",
            Self::Completed => "Completed",
        }
    }

    fn next(self) -> Option<Self> {
        Some(match self {
            Self::Proposing => Self::Critiquing,
            Self::Critiquing => Self::Revising,
            Self::Revising => Self::Validating,
            Self::Validating => Self::Scoring,
            Self::Scoring => Self::Completed,
            Self::Completed => return None,
        })
    }
}

/// A proposal paired with its validation outcome and final rank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankedOutcome {
    proposal: Proposal,
    outcome: ValidationOutcome,
    rank: u32,
}

impl RankedOutcome {
    #[must_use]
    pub fn proposal(&self) -> &Proposal {
        &self.proposal
    }
    #[must_use]
    pub fn outcome(&self) -> &ValidationOutcome {
        &self.outcome
    }
    #[must_use]
    pub fn rank(&self) -> u32 {
        self.rank
    }
}

/// Aggregate root: one deliberation over one task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deliberation {
    task_id: TaskId,
    specialty: Specialty,
    rounds_budget: Rounds,
    phase: DeliberationPhase,

    proposals: BTreeMap<ProposalId, Proposal>,
    outcomes: BTreeMap<ProposalId, ValidationOutcome>,
    ranking: Vec<ProposalId>,

    #[serde(with = "time::serde::rfc3339")]
    started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    completed_at: Option<OffsetDateTime>,
}

impl Deliberation {
    #[must_use]
    pub fn start(
        task_id: TaskId,
        specialty: Specialty,
        rounds_budget: Rounds,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            task_id,
            specialty,
            rounds_budget,
            phase: DeliberationPhase::Proposing,
            proposals: BTreeMap::new(),
            outcomes: BTreeMap::new(),
            ranking: Vec::new(),
            started_at: now,
            completed_at: None,
        }
    }

    #[must_use]
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    #[must_use]
    pub fn specialty(&self) -> &Specialty {
        &self.specialty
    }
    #[must_use]
    pub fn rounds_budget(&self) -> Rounds {
        self.rounds_budget
    }
    #[must_use]
    pub fn phase(&self) -> DeliberationPhase {
        self.phase
    }
    #[must_use]
    pub fn proposals(&self) -> &BTreeMap<ProposalId, Proposal> {
        &self.proposals
    }
    #[must_use]
    pub fn outcomes(&self) -> &BTreeMap<ProposalId, ValidationOutcome> {
        &self.outcomes
    }
    #[must_use]
    pub fn started_at(&self) -> OffsetDateTime {
        self.started_at
    }
    #[must_use]
    pub fn completed_at(&self) -> Option<OffsetDateTime> {
        self.completed_at
    }

    /// Add a new proposal. Only allowed while `Proposing`. Duplicate
    /// proposal ids are rejected.
    pub fn add_proposal(&mut self, proposal: Proposal) -> Result<(), DomainError> {
        self.require_phase(DeliberationPhase::Proposing)?;
        if self.proposals.contains_key(proposal.id()) {
            return Err(DomainError::AlreadyExists {
                what: "deliberation.proposal",
            });
        }
        self.proposals.insert(proposal.id().clone(), proposal);
        Ok(())
    }

    /// Revise an existing proposal. Only allowed in the `Revising` phase.
    pub fn revise_proposal(
        &mut self,
        proposal_id: &ProposalId,
        new_content: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.require_phase(DeliberationPhase::Revising)?;
        let proposal = self
            .proposals
            .get_mut(proposal_id)
            .ok_or(DomainError::NotFound {
                what: "deliberation.proposal",
            })?;
        proposal.revise(new_content, now)
    }

    /// Attach a validation outcome for a proposal. Only allowed in
    /// `Validating`. Every proposal must receive exactly one outcome
    /// before advancing to `Scoring`.
    pub fn attach_outcome(
        &mut self,
        proposal_id: &ProposalId,
        outcome: ValidationOutcome,
    ) -> Result<(), DomainError> {
        self.require_phase(DeliberationPhase::Validating)?;
        if !self.proposals.contains_key(proposal_id) {
            return Err(DomainError::NotFound {
                what: "deliberation.proposal",
            });
        }
        if self.outcomes.contains_key(proposal_id) {
            return Err(DomainError::AlreadyExists {
                what: "deliberation.outcome",
            });
        }
        self.outcomes.insert(proposal_id.clone(), outcome);
        Ok(())
    }

    /// Advance to the next phase, enforcing the preconditions of the
    /// transition:
    ///
    /// - `Proposing -> Critiquing`: at least one proposal present.
    /// - `Validating -> Scoring`: every proposal has an outcome.
    /// - Other transitions are unconditional.
    pub fn advance(&mut self) -> Result<DeliberationPhase, DomainError> {
        let next = self.phase.next().ok_or(DomainError::InvalidTransition {
            from: "Completed",
            to: "Completed",
        })?;

        match (self.phase, next) {
            (DeliberationPhase::Proposing, DeliberationPhase::Critiquing) => {
                if self.proposals.is_empty() {
                    return Err(DomainError::InvariantViolated {
                        reason: "cannot leave Proposing without proposals",
                    });
                }
            }
            (DeliberationPhase::Validating, DeliberationPhase::Scoring) => {
                if self.outcomes.len() != self.proposals.len() {
                    return Err(DomainError::InvariantViolated {
                        reason: "every proposal must have an outcome before Scoring",
                    });
                }
            }
            _ => {}
        }

        self.phase = next;
        Ok(self.phase)
    }

    /// Compute the ranking and mark the deliberation complete. Only
    /// allowed from `Scoring`. The winning proposal gets rank 0; ties
    /// are broken by proposal id to keep the ordering deterministic.
    pub fn complete(&mut self, now: OffsetDateTime) -> Result<Vec<RankedOutcome>, DomainError> {
        self.require_phase(DeliberationPhase::Scoring)?;

        let mut ranked: Vec<(ProposalId, Proposal, ValidationOutcome)> = self
            .proposals
            .iter()
            .map(|(id, proposal)| {
                let outcome =
                    self.outcomes
                        .get(id)
                        .cloned()
                        .ok_or(DomainError::InvariantViolated {
                            reason: "missing outcome at Scoring",
                        })?;
                Ok::<_, DomainError>((id.clone(), proposal.clone(), outcome))
            })
            .collect::<Result<_, _>>()?;

        ranked.sort_by(|a, b| b.2.score().cmp(&a.2.score()).then_with(|| a.0.cmp(&b.0)));

        self.ranking = ranked.iter().map(|(id, _, _)| id.clone()).collect();
        self.phase = DeliberationPhase::Completed;
        self.completed_at = Some(now);

        Ok(ranked
            .into_iter()
            .enumerate()
            .map(|(i, (_, proposal, outcome))| RankedOutcome {
                proposal,
                outcome,
                rank: u32::try_from(i).unwrap_or(u32::MAX),
            })
            .collect())
    }

    /// Total duration from start to completion, when completed.
    #[must_use]
    pub fn duration(&self) -> Option<DurationMs> {
        self.completed_at.map(|end| {
            let delta = end - self.started_at;
            let millis = delta.whole_milliseconds();
            let bounded = u64::try_from(millis).unwrap_or(0);
            DurationMs::from_millis(bounded)
        })
    }

    #[must_use]
    pub fn ranking(&self) -> &[ProposalId] {
        &self.ranking
    }

    fn require_phase(&self, expected: DeliberationPhase) -> Result<(), DomainError> {
        if self.phase == expected {
            Ok(())
        } else {
            Err(DomainError::InvalidTransition {
                from: self.phase.name(),
                to: expected.name(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::validation::ValidatorReport;
    use crate::value_objects::{AgentId, Attributes, Score, TaskId};
    use time::macros::datetime;

    fn now() -> OffsetDateTime {
        datetime!(2026-04-15 12:00:00 UTC)
    }

    fn specialty() -> Specialty {
        Specialty::new("triage").unwrap()
    }

    fn start() -> Deliberation {
        Deliberation::start(
            TaskId::new("t1").unwrap(),
            specialty(),
            Rounds::default(),
            now(),
        )
    }

    fn proposal(id: &str, content: &str) -> Proposal {
        Proposal::new(
            ProposalId::new(id).unwrap(),
            AgentId::new("a").unwrap(),
            specialty(),
            content,
            Attributes::empty(),
            now(),
        )
        .unwrap()
    }

    fn outcome(score: f64) -> ValidationOutcome {
        ValidationOutcome::new(
            Score::new(score).unwrap(),
            vec![ValidatorReport::new("x", true, "", Attributes::empty()).unwrap()],
        )
    }

    #[test]
    fn starts_in_proposing() {
        let d = start();
        assert_eq!(d.phase(), DeliberationPhase::Proposing);
        assert!(d.proposals().is_empty());
        assert!(d.completed_at().is_none());
    }

    #[test]
    fn proposals_only_accepted_while_proposing() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        d.advance().unwrap(); // Critiquing
        let err = d.add_proposal(proposal("p2", "y")).unwrap_err();
        assert!(matches!(err, DomainError::InvalidTransition { .. }));
    }

    #[test]
    fn duplicate_proposal_id_is_rejected() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        assert!(matches!(
            d.add_proposal(proposal("p1", "y")).unwrap_err(),
            DomainError::AlreadyExists { .. }
        ));
    }

    #[test]
    fn cannot_leave_proposing_without_proposals() {
        let mut d = start();
        assert!(matches!(
            d.advance().unwrap_err(),
            DomainError::InvariantViolated { .. }
        ));
        assert_eq!(d.phase(), DeliberationPhase::Proposing);
    }

    #[test]
    fn revise_only_allowed_while_revising() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        assert!(matches!(
            d.revise_proposal(&ProposalId::new("p1").unwrap(), "y", now())
                .unwrap_err(),
            DomainError::InvalidTransition { .. }
        ));
        d.advance().unwrap(); // Critiquing
        d.advance().unwrap(); // Revising
        d.revise_proposal(&ProposalId::new("p1").unwrap(), "y", now())
            .unwrap();
        assert_eq!(
            d.proposals()
                .get(&ProposalId::new("p1").unwrap())
                .unwrap()
                .content(),
            "y"
        );
    }

    #[test]
    fn cannot_enter_scoring_with_missing_outcomes() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        d.add_proposal(proposal("p2", "y")).unwrap();
        for _ in 0..3 {
            d.advance().unwrap();
        }
        // Now in Validating. Attach only one outcome.
        d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.9))
            .unwrap();
        assert!(matches!(
            d.advance().unwrap_err(),
            DomainError::InvariantViolated { .. }
        ));
    }

    #[test]
    fn duplicate_outcome_is_rejected() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        for _ in 0..3 {
            d.advance().unwrap();
        }
        d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.8))
            .unwrap();
        assert!(matches!(
            d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.9))
                .unwrap_err(),
            DomainError::AlreadyExists { .. }
        ));
    }

    #[test]
    fn complete_ranks_descending_by_score() {
        let mut d = start();
        d.add_proposal(proposal("p1", "a")).unwrap();
        d.add_proposal(proposal("p2", "b")).unwrap();
        d.add_proposal(proposal("p3", "c")).unwrap();
        for _ in 0..3 {
            d.advance().unwrap();
        }
        d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.5))
            .unwrap();
        d.attach_outcome(&ProposalId::new("p2").unwrap(), outcome(0.9))
            .unwrap();
        d.attach_outcome(&ProposalId::new("p3").unwrap(), outcome(0.7))
            .unwrap();
        d.advance().unwrap(); // Scoring

        let ranked = d.complete(datetime!(2026-04-15 12:00:01 UTC)).unwrap();
        assert_eq!(d.phase(), DeliberationPhase::Completed);
        assert_eq!(ranked[0].rank(), 0);
        assert_eq!(ranked[0].proposal().id().as_str(), "p2");
        assert_eq!(ranked[1].proposal().id().as_str(), "p3");
        assert_eq!(ranked[2].proposal().id().as_str(), "p1");
    }

    #[test]
    fn ties_are_broken_by_proposal_id() {
        let mut d = start();
        d.add_proposal(proposal("p2", "a")).unwrap();
        d.add_proposal(proposal("p1", "b")).unwrap();
        for _ in 0..3 {
            d.advance().unwrap();
        }
        d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.7))
            .unwrap();
        d.attach_outcome(&ProposalId::new("p2").unwrap(), outcome(0.7))
            .unwrap();
        d.advance().unwrap();

        let ranked = d.complete(now()).unwrap();
        assert_eq!(ranked[0].proposal().id().as_str(), "p1");
        assert_eq!(ranked[1].proposal().id().as_str(), "p2");
    }

    #[test]
    fn complete_only_allowed_from_scoring() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        assert!(matches!(
            d.complete(now()).unwrap_err(),
            DomainError::InvalidTransition { .. }
        ));
    }

    #[test]
    fn completed_deliberation_has_duration() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        for _ in 0..3 {
            d.advance().unwrap();
        }
        d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.5))
            .unwrap();
        d.advance().unwrap();
        d.complete(datetime!(2026-04-15 12:00:00.750 UTC)).unwrap();

        assert_eq!(d.duration().unwrap().get(), 750);
    }

    #[test]
    fn cannot_advance_past_completed() {
        let mut d = start();
        d.add_proposal(proposal("p1", "x")).unwrap();
        for _ in 0..3 {
            d.advance().unwrap();
        }
        d.attach_outcome(&ProposalId::new("p1").unwrap(), outcome(0.5))
            .unwrap();
        d.advance().unwrap();
        d.complete(now()).unwrap();
        assert!(matches!(
            d.advance().unwrap_err(),
            DomainError::InvalidTransition { .. }
        ));
    }
}
