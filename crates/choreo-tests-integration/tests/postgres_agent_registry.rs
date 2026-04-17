//! Integration test: [`PostgresAgentRegistry`] exercises
//! `AgentRegistryPort` (write) + `AgentResolverPort` (read, via
//! factory rehydration) against a real Postgres container.
//!
//! Runs only when the `container-tests` feature is enabled (CI).

#![cfg(feature = "container-tests")]

use std::sync::Arc;

use choreo_adapters::noop::NoopAgentFactory;
use choreo_adapters::postgres::PostgresAgentRegistry;
use choreo_core::entities::TaskConstraints;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    AgentDescriptor, AgentFactoryPort, AgentPort, AgentRegistryPort, AgentResolverPort, Critique,
    DraftRequest, Revision,
};
use choreo_core::value_objects::{AgentId, AgentKind, Attributes, Specialty};
use choreo_tests_integration::postgres_fixture;

fn factory() -> Arc<dyn AgentFactoryPort> {
    Arc::new(NoopAgentFactory::new())
}

fn noop_descriptor(id: &str, specialty: &str) -> AgentDescriptor {
    AgentDescriptor {
        id: AgentId::new(id).unwrap(),
        specialty: Specialty::new(specialty).unwrap(),
        kind: AgentKind::new("noop").unwrap(),
        attributes: Attributes::empty(),
    }
}

#[derive(Debug)]
struct StubAgent {
    id: AgentId,
    specialty: Specialty,
}
#[async_trait::async_trait]
impl AgentPort for StubAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }
    fn specialty(&self) -> &Specialty {
        &self.specialty
    }
    async fn generate(&self, _: DraftRequest) -> Result<Revision, DomainError> {
        Ok(Revision {
            content: String::new(),
        })
    }
    async fn critique(&self, _: &str, _: &TaskConstraints) -> Result<Critique, DomainError> {
        Ok(Critique {
            feedback: String::new(),
        })
    }
    async fn revise(&self, own: &str, _: &Critique) -> Result<Revision, DomainError> {
        Ok(Revision {
            content: own.to_owned(),
        })
    }
}

#[tokio::test]
async fn insert_descriptor_roundtrips_through_resolve() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresAgentRegistry::new(pool, factory());

    registry
        .insert_descriptor(&noop_descriptor("a1", "triage"))
        .await
        .unwrap();

    let resolved = registry
        .resolve(&AgentId::new("a1").unwrap())
        .await
        .unwrap();
    assert_eq!(resolved.id().as_str(), "a1");
    assert_eq!(resolved.specialty().as_str(), "triage");
}

#[tokio::test]
async fn duplicate_insert_is_already_exists() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresAgentRegistry::new(pool, factory());

    registry
        .insert_descriptor(&noop_descriptor("a1", "triage"))
        .await
        .unwrap();
    let err = registry
        .insert_descriptor(&noop_descriptor("a1", "triage"))
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::AlreadyExists { what: "agent" }));
}

#[tokio::test]
async fn register_via_port_trait_persists_then_unregister_removes() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresAgentRegistry::new(pool, factory());

    let agent: Arc<dyn AgentPort> = Arc::new(StubAgent {
        id: AgentId::new("a-port").unwrap(),
        specialty: Specialty::new("triage").unwrap(),
    });
    registry.register(agent).await.unwrap();
    // Post-register the descriptor resolves to a live NoopAgent (the
    // wired factory only knows the "noop" kind; the port contract
    // does not expose which concrete type is returned).
    let resolved = registry
        .resolve(&AgentId::new("a-port").unwrap())
        .await
        .unwrap();
    assert_eq!(resolved.id().as_str(), "a-port");

    registry
        .unregister(&AgentId::new("a-port").unwrap())
        .await
        .unwrap();
    let err = registry
        .resolve(&AgentId::new("a-port").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::NotFound { what: "agent" }));
}

#[tokio::test]
async fn unregister_missing_is_not_found() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresAgentRegistry::new(pool, factory());

    let err = registry
        .unregister(&AgentId::new("ghost").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::NotFound { what: "agent" }));
}

#[tokio::test]
async fn resolve_propagates_factory_rejection_for_unsupported_kind() {
    let (pool, _container) = postgres_fixture::start().await;
    let registry = PostgresAgentRegistry::new(pool, factory());

    let descriptor = AgentDescriptor {
        id: AgentId::new("a-vllm").unwrap(),
        specialty: Specialty::new("triage").unwrap(),
        kind: AgentKind::new("vllm").unwrap(),
        attributes: Attributes::empty(),
    };
    registry.insert_descriptor(&descriptor).await.unwrap();

    // The noop factory rejects every non-"noop" kind — a deliberate
    // honesty signal that the deployment's factory is not wired for
    // the registered kind.
    let err = registry
        .resolve(&AgentId::new("a-vllm").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, DomainError::InvariantViolated { .. }));
}
