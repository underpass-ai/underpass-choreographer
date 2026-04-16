//! Deliberation output flattening.
//!
//! The core returns a full `Deliberation` aggregate plus a winner id;
//! the transport shape is flat (ranked results, winner id, duration,
//! metadata). This mapper bridges the two without letting the core
//! know the wire shape exists.

use choreo_app::usecases::{DeliberateOutput, OrchestrateOutput};
use choreo_proto::v1 as pb;
use prost_types::Struct as PbStruct;

use super::proposal::proposal_to_proto;
use super::validation::validation_outcome_to_proto;

#[must_use]
pub fn deliberate_response_from(out: &DeliberateOutput) -> pb::DeliberateResponse {
    let results = flatten_ranked(&out.deliberation);
    let duration_ms = out
        .deliberation
        .duration()
        .map_or(0, choreo_core::value_objects::DurationMs::get);

    pb::DeliberateResponse {
        task_id: out.deliberation.task_id().as_str().to_owned(),
        results,
        winner_proposal_id: out.winner_proposal_id.as_str().to_owned(),
        duration_ms,
        metadata: Some(PbStruct::default()),
    }
}

#[must_use]
pub fn orchestrate_response_from(out: &OrchestrateOutput) -> pb::OrchestrateResponse {
    let mut results = flatten_ranked(&out.deliberation);
    let winner_proposal_id = out.winner.id().as_str().to_owned();
    let winner = results
        .iter()
        .find(|r| {
            r.proposal
                .as_ref()
                .is_some_and(|p| p.proposal_id == winner_proposal_id)
        })
        .cloned();
    results.retain(|r| {
        r.proposal
            .as_ref()
            .is_none_or(|p| p.proposal_id != winner_proposal_id)
    });

    pb::OrchestrateResponse {
        task_id: out.deliberation.task_id().as_str().to_owned(),
        execution_id: out.execution.execution_id.clone(),
        winner,
        candidates: results,
        duration_ms: out.execution.duration.get(),
        metadata: Some(PbStruct::default()),
    }
}

fn flatten_ranked(
    deliberation: &choreo_core::entities::Deliberation,
) -> Vec<pb::DeliberationResult> {
    let ranking = deliberation.ranking();
    if ranking.is_empty() {
        return Vec::new();
    }
    let proposals = deliberation.proposals();
    let outcomes = deliberation.outcomes();
    ranking
        .iter()
        .enumerate()
        .filter_map(|(rank, id)| {
            let proposal = proposals.get(id)?;
            let outcome = outcomes.get(id)?;
            Some(pb::DeliberationResult {
                proposal: Some(proposal_to_proto(proposal)),
                validation: Some(validation_outcome_to_proto(outcome)),
                rank: u32::try_from(rank).unwrap_or(u32::MAX),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::entities::{
        Deliberation, Proposal, TaskConstraints, ValidationOutcome, ValidatorReport,
    };
    use choreo_core::ports::ExecutionOutcome;
    use choreo_core::value_objects::{
        AgentId, Attributes, DurationMs, ProposalId, Rounds, Score, Specialty, TaskDescription,
        TaskId,
    };
    use time::macros::datetime;

    fn completed_deliberation() -> (Deliberation, ProposalId) {
        let now = datetime!(2026-04-15 12:00:00 UTC);
        let specialty = Specialty::new("triage").unwrap();
        let mut d = Deliberation::start(
            TaskId::new("t1").unwrap(),
            specialty.clone(),
            Rounds::default(),
            now,
        );
        for (i, content) in [("p1", "a"), ("p2", "b")] {
            let _ = content;
            let p = Proposal::new(
                ProposalId::new(i).unwrap(),
                AgentId::new(i).unwrap(),
                specialty.clone(),
                format!("content-{i}"),
                Attributes::empty(),
                now,
            )
            .unwrap();
            d.add_proposal(p).unwrap();
        }
        for _ in 0..2 {
            d.advance().unwrap();
        }
        let winner_score = Score::new(0.9).unwrap();
        let loser_score = Score::new(0.3).unwrap();
        let report = |passed| ValidatorReport::new("v", passed, "ok", Attributes::empty()).unwrap();
        d.attach_outcome(
            &ProposalId::new("p1").unwrap(),
            ValidationOutcome::new(loser_score, vec![report(false)]),
        )
        .unwrap();
        d.attach_outcome(
            &ProposalId::new("p2").unwrap(),
            ValidationOutcome::new(winner_score, vec![report(true)]),
        )
        .unwrap();
        d.advance().unwrap();
        let later = datetime!(2026-04-15 12:00:00.500 UTC);
        d.complete(later).unwrap();
        (d, ProposalId::new("p2").unwrap())
    }

    fn _unused() -> Option<TaskConstraints> {
        // Keep the `TaskConstraints` import visible for readers of
        // this test file; not actually called at runtime.
        None
    }

    #[test]
    fn deliberate_response_lists_ranked_results_winner_first() {
        let (d, winner) = completed_deliberation();
        let out = DeliberateOutput {
            deliberation: d,
            winner_proposal_id: winner.clone(),
        };
        let resp = deliberate_response_from(&out);
        assert_eq!(resp.task_id, "t1");
        assert_eq!(resp.winner_proposal_id, winner.as_str());
        assert_eq!(resp.results.len(), 2);
        assert_eq!(resp.results[0].rank, 0);
        assert_eq!(
            resp.results[0].proposal.as_ref().unwrap().proposal_id,
            winner.as_str()
        );
        assert!(resp.duration_ms > 0);
    }

    #[test]
    fn orchestrate_response_separates_winner_from_candidates() {
        let (d, winner_id) = completed_deliberation();
        let winner_proposal = d.proposals().get(&winner_id).unwrap().clone();
        let out = OrchestrateOutput {
            deliberation: d,
            winner: winner_proposal,
            execution: ExecutionOutcome {
                execution_id: "e-1".to_owned(),
                succeeded: true,
                duration: DurationMs::from_millis(250),
                output: Attributes::empty(),
            },
        };
        let resp = orchestrate_response_from(&out);
        assert_eq!(resp.execution_id, "e-1");
        assert_eq!(resp.duration_ms, 250);
        let winner = resp.winner.unwrap();
        assert_eq!(winner.proposal.unwrap().proposal_id, winner_id.as_str());
        assert_eq!(resp.candidates.len(), 1);
        assert_ne!(
            resp.candidates[0].proposal.as_ref().unwrap().proposal_id,
            winner_id.as_str()
        );
    }

    #[test]
    fn keep_task_description_imported() {
        // Read-only anchor so the `TaskDescription` import stays
        // meaningful to a reader of this test file.
        let _ = TaskDescription::new("x").unwrap();
    }
}
