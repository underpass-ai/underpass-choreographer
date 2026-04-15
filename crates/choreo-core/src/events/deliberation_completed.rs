//! [`DeliberationCompletedEvent`] — a deliberation finished with a winner.

use serde::{Deserialize, Serialize};

use crate::events::envelope::EventEnvelope;
use crate::value_objects::{DurationMs, ProposalId, Score, Specialty, TaskId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliberationCompletedEvent {
    envelope: EventEnvelope,
    task_id: TaskId,
    specialty: Specialty,
    winner_proposal_id: ProposalId,
    winner_score: Score,
    num_candidates: u32,
    duration: DurationMs,
}

impl DeliberationCompletedEvent {
    #[must_use]
    pub fn new(
        envelope: EventEnvelope,
        task_id: TaskId,
        specialty: Specialty,
        winner_proposal_id: ProposalId,
        winner_score: Score,
        num_candidates: u32,
        duration: DurationMs,
    ) -> Self {
        Self {
            envelope,
            task_id,
            specialty,
            winner_proposal_id,
            winner_score,
            num_candidates,
            duration,
        }
    }

    #[must_use]
    pub fn envelope(&self) -> &EventEnvelope {
        &self.envelope
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
    pub fn winner_proposal_id(&self) -> &ProposalId {
        &self.winner_proposal_id
    }
    #[must_use]
    pub fn winner_score(&self) -> Score {
        self.winner_score
    }
    #[must_use]
    pub fn num_candidates(&self) -> u32 {
        self.num_candidates
    }
    #[must_use]
    pub fn duration(&self) -> DurationMs {
        self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::EventId;
    use time::macros::datetime;

    #[test]
    fn accessors_return_fields() {
        let env = EventEnvelope::new(
            EventId::new("e").unwrap(),
            datetime!(2026-04-15 12:00:00 UTC),
            "s",
            None,
        )
        .unwrap();
        let ev = DeliberationCompletedEvent::new(
            env,
            TaskId::new("t").unwrap(),
            Specialty::new("triage").unwrap(),
            ProposalId::new("p").unwrap(),
            Score::new(0.87).unwrap(),
            3,
            DurationMs::from_millis(900),
        );
        assert_eq!(ev.num_candidates(), 3);
        assert_eq!(ev.winner_score().get(), 0.87);
        assert_eq!(ev.duration().get(), 900);
    }
}
