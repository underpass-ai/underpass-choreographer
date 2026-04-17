//! Integration test: [`PostgresStatistics`] against a real Postgres
//! container. Exercises record / snapshot semantics and the
//! concurrent-accumulation guarantee the counter-increment UPSERT
//! protocol is meant to provide.
//!
//! Runs only when the `container-tests` feature is enabled (CI).

#![cfg(feature = "container-tests")]

use std::sync::Arc;

use choreo_adapters::postgres::PostgresStatistics;
use choreo_core::ports::StatisticsPort;
use choreo_core::value_objects::{DurationMs, Specialty};
use choreo_tests_integration::postgres_fixture;
use futures::future::join_all;

#[tokio::test]
async fn fresh_snapshot_is_empty() {
    let (pool, _container) = postgres_fixture::start().await;
    let stats = PostgresStatistics::new(pool);
    let snap = stats.snapshot().await.unwrap();
    assert_eq!(snap.total_deliberations(), 0);
    assert_eq!(snap.total_orchestrations(), 0);
    assert_eq!(snap.total_duration(), DurationMs::ZERO);
    assert!(snap.per_specialty().is_empty());
}

#[tokio::test]
async fn record_deliberation_populates_totals_and_per_specialty() {
    let (pool, _container) = postgres_fixture::start().await;
    let stats = PostgresStatistics::new(pool);

    let triage = Specialty::new("triage").unwrap();
    let reviewer = Specialty::new("reviewer").unwrap();

    stats
        .record_deliberation(&triage, DurationMs::from_millis(100))
        .await
        .unwrap();
    stats
        .record_deliberation(&triage, DurationMs::from_millis(50))
        .await
        .unwrap();
    stats
        .record_deliberation(&reviewer, DurationMs::from_millis(200))
        .await
        .unwrap();

    let snap = stats.snapshot().await.unwrap();
    assert_eq!(snap.total_deliberations(), 3);
    assert_eq!(snap.total_orchestrations(), 0);
    assert_eq!(snap.total_duration(), DurationMs::from_millis(350));
    assert_eq!(snap.per_specialty().get(&triage).copied(), Some(2));
    assert_eq!(snap.per_specialty().get(&reviewer).copied(), Some(1));
}

#[tokio::test]
async fn record_orchestration_does_not_touch_per_specialty() {
    let (pool, _container) = postgres_fixture::start().await;
    let stats = PostgresStatistics::new(pool);

    stats
        .record_orchestration(DurationMs::from_millis(400))
        .await
        .unwrap();
    let snap = stats.snapshot().await.unwrap();
    assert_eq!(snap.total_orchestrations(), 1);
    assert_eq!(snap.total_duration(), DurationMs::from_millis(400));
    assert!(snap.per_specialty().is_empty());
}

#[tokio::test]
async fn concurrent_records_accumulate_without_loss() {
    // The UPSERT-with-increment protocol is the core design decision
    // for multi-replica safety. This test exercises it by firing 50
    // concurrent record_deliberation calls against the same row and
    // asserting the counter matches the number of requests.
    let (pool, _container) = postgres_fixture::start().await;
    let stats = Arc::new(PostgresStatistics::new(pool));
    let triage = Specialty::new("triage").unwrap();

    let tasks = (0..50).map(|_| {
        let stats = stats.clone();
        let triage = triage.clone();
        async move {
            stats
                .record_deliberation(&triage, DurationMs::from_millis(10))
                .await
                .unwrap();
        }
    });
    join_all(tasks).await;

    let snap = stats.snapshot().await.unwrap();
    assert_eq!(
        snap.total_deliberations(),
        50,
        "every concurrent record must be observed"
    );
    assert_eq!(snap.per_specialty().get(&triage).copied(), Some(50));
    assert_eq!(snap.total_duration(), DurationMs::from_millis(500));
}
