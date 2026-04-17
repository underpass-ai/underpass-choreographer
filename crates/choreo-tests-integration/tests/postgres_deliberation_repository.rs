//! Integration test: [`PostgresDeliberationRepository`] roundtrips a
//! real, non-trivial [`Deliberation`] through a real Postgres container.
//!
//! Exercises:
//!   1. Pool dial + migration runner.
//!   2. `save` as upsert (insert + overwrite paths).
//!   3. `get` returns a structurally equal aggregate after JSONB
//!      serialisation and rehydration.
//!   4. `exists` reflects presence / absence truthfully.
//!   5. `get` for a missing task id surfaces `DomainError::NotFound`.
//!
//! Runs only when the `container-tests` feature is enabled (CI).

#![cfg(feature = "container-tests")]

use choreo_adapters::postgres::PostgresDeliberationRepository;
use choreo_core::entities::{
    Deliberation, DeliberationPhase, Proposal, ValidationOutcome, ValidatorReport,
};
use choreo_core::error::DomainError;
use choreo_core::ports::DeliberationRepositoryPort;
use choreo_core::value_objects::{
    AgentId, Attributes, ProposalId, Rounds, Score, Specialty, TaskId,
};
use choreo_tests_integration::postgres_fixture;
use time::macros::datetime;

/// Build a fully-traversed Deliberation so the roundtrip covers
/// proposals, outcomes, ranking, and the completed phase (the hard
/// cases for JSONB serialisation).
fn completed_deliberation(task: &str) -> Deliberation {
    let now = datetime!(2026-04-15 12:00:00 UTC);
    let specialty = Specialty::new("triage").unwrap();
    let mut d = Deliberation::start(
        TaskId::new(task).unwrap(),
        specialty.clone(),
        Rounds::default(),
        now,
    );
    for id in ["p1", "p2"] {
        let p = Proposal::new(
            ProposalId::new(id).unwrap(),
            AgentId::new(id).unwrap(),
            specialty.clone(),
            format!("content-{id}"),
            Attributes::empty(),
            now,
        )
        .unwrap();
        d.add_proposal(p).unwrap();
    }
    for _ in 0..2 {
        d.advance().unwrap();
    }
    let report = |passed| ValidatorReport::new("v", passed, "ok", Attributes::empty()).unwrap();
    d.attach_outcome(
        &ProposalId::new("p1").unwrap(),
        ValidationOutcome::new(Score::new(0.3).unwrap(), vec![report(false)]),
    )
    .unwrap();
    d.attach_outcome(
        &ProposalId::new("p2").unwrap(),
        ValidationOutcome::new(Score::new(0.9).unwrap(), vec![report(true)]),
    )
    .unwrap();
    d.advance().unwrap();
    let later = datetime!(2026-04-15 12:00:00.500 UTC);
    d.complete(later).unwrap();
    d
}

#[tokio::test]
async fn postgres_repository_roundtrips_a_completed_deliberation() {
    let (pool, _container) = postgres_fixture::start().await;
    let repo = PostgresDeliberationRepository::new(pool);

    let d = completed_deliberation("t-happy");
    repo.save(&d).await.unwrap();

    let got = repo.get(d.task_id()).await.unwrap();
    assert_eq!(got.task_id(), d.task_id());
    assert_eq!(got.specialty(), d.specialty());
    assert_eq!(got.phase(), DeliberationPhase::Completed);
    assert_eq!(got.proposals().len(), d.proposals().len());
    assert_eq!(got.outcomes().len(), d.outcomes().len());
    assert_eq!(got.ranking(), d.ranking());
    // The winner (p2) is ranked first.
    assert_eq!(
        got.ranking().first().map(ProposalId::as_str),
        Some("p2"),
        "winner must round-trip with the same ranking"
    );
}

#[tokio::test]
async fn save_is_upsert_and_preserves_the_latest_body() {
    let (pool, _container) = postgres_fixture::start().await;
    let repo = PostgresDeliberationRepository::new(pool);

    // First save — a mid-run deliberation (phase Proposing).
    let mut d = Deliberation::start(
        TaskId::new("t-upsert").unwrap(),
        Specialty::new("triage").unwrap(),
        Rounds::default(),
        datetime!(2026-04-15 12:00:00 UTC),
    );
    d.add_proposal(
        Proposal::new(
            ProposalId::new("p1").unwrap(),
            AgentId::new("a1").unwrap(),
            Specialty::new("triage").unwrap(),
            "initial-content".to_owned(),
            Attributes::empty(),
            datetime!(2026-04-15 12:00:00 UTC),
        )
        .unwrap(),
    )
    .unwrap();
    repo.save(&d).await.unwrap();
    let first = repo.get(d.task_id()).await.unwrap();
    assert_eq!(first.phase(), DeliberationPhase::Proposing);

    // Second save — a fully completed deliberation for the same id.
    let completed = completed_deliberation("t-upsert");
    repo.save(&completed).await.unwrap();
    let second = repo.get(completed.task_id()).await.unwrap();
    assert_eq!(
        second.phase(),
        DeliberationPhase::Completed,
        "later save must overwrite the earlier body"
    );
    assert_eq!(second.proposals().len(), 2);
}

#[tokio::test]
async fn exists_reflects_presence_and_missing_get_is_not_found() {
    let (pool, _container) = postgres_fixture::start().await;
    let repo = PostgresDeliberationRepository::new(pool);

    let id = TaskId::new("t-absent").unwrap();
    assert!(!repo.exists(&id).await.unwrap());

    let err = repo.get(&id).await.unwrap_err();
    assert!(
        matches!(
            err,
            DomainError::NotFound {
                what: "deliberation"
            }
        ),
        "missing task must surface as NotFound, got {err:?}"
    );

    let d = completed_deliberation("t-absent");
    repo.save(&d).await.unwrap();
    assert!(repo.exists(&id).await.unwrap());
}
