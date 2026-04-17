//! Integration test: [`PostgresCouncilRegistry`] exercises the full
//! write + read surface against a real Postgres container.
//!
//! Covers: register (insert path + duplicate rejection), replace
//! (update path + missing-specialty rejection), get, list, delete,
//! contains.
//!
//! Runs only when the `container-tests` feature is enabled (CI).

#![cfg(feature = "container-tests")]

use choreo_adapters::postgres::PostgresCouncilRegistry;
use choreo_core::entities::Council;
use choreo_core::error::DomainError;
use choreo_core::ports::CouncilRegistryPort;
use choreo_core::value_objects::{AgentId, CouncilId, Specialty};
use choreo_tests_integration::postgres_fixture;
use time::macros::datetime;

fn council(specialty: &str, council_id: &str, agents: &[&str]) -> Council {
    let agent_ids: Vec<AgentId> = agents.iter().map(|a| AgentId::new(*a).unwrap()).collect();
    Council::new(
        CouncilId::new(council_id).unwrap(),
        Specialty::new(specialty).unwrap(),
        agent_ids,
        datetime!(2026-04-15 12:00:00 UTC),
    )
    .unwrap()
}

#[tokio::test]
async fn register_roundtrips_through_get_and_list() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresCouncilRegistry::new(pool);

    registry
        .register(council("triage", "c-triage", &["a1"]))
        .await
        .unwrap();
    registry
        .register(council("reviewer", "c-reviewer", &["a2", "a3"]))
        .await
        .unwrap();

    let fetched = registry
        .get(&Specialty::new("triage").unwrap())
        .await
        .unwrap();
    assert_eq!(fetched.specialty().as_str(), "triage");
    assert_eq!(fetched.size(), 1);
    assert_eq!(fetched.id().as_str(), "c-triage");

    let listed = registry.list().await.unwrap();
    assert_eq!(listed.len(), 2);
    // Ordering follows the index on `specialty`.
    assert_eq!(listed[0].specialty().as_str(), "reviewer");
    assert_eq!(listed[1].specialty().as_str(), "triage");
}

#[tokio::test]
async fn duplicate_register_surfaces_as_already_exists() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresCouncilRegistry::new(pool);

    registry
        .register(council("triage", "c1", &["a1"]))
        .await
        .unwrap();
    let err = registry
        .register(council("triage", "c2", &["a2"]))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        DomainError::AlreadyExists { what: "council" }
    ));
}

#[tokio::test]
async fn replace_updates_only_when_specialty_exists() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresCouncilRegistry::new(pool);

    // Replacing a missing specialty is NotFound.
    let err = registry
        .replace(council("triage", "c1", &["a1"]))
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::NotFound { what: "council" }));

    // After register, replace updates in place.
    registry
        .register(council("triage", "c1", &["a1"]))
        .await
        .unwrap();
    registry
        .replace(council("triage", "c2", &["a1", "a2"]))
        .await
        .unwrap();
    let got = registry
        .get(&Specialty::new("triage").unwrap())
        .await
        .unwrap();
    assert_eq!(got.id().as_str(), "c2", "replace must persist the new id");
    assert_eq!(got.size(), 2);
}

#[tokio::test]
async fn delete_and_contains_reflect_presence() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresCouncilRegistry::new(pool);

    let triage = Specialty::new("triage").unwrap();
    assert!(!registry.contains(&triage).await.unwrap());

    registry
        .register(council("triage", "c1", &["a1"]))
        .await
        .unwrap();
    assert!(registry.contains(&triage).await.unwrap());

    registry.delete(&triage).await.unwrap();
    assert!(!registry.contains(&triage).await.unwrap());

    // Deleting twice surfaces NotFound.
    let err = registry.delete(&triage).await.unwrap_err();
    assert!(matches!(err, DomainError::NotFound { what: "council" }));
}
