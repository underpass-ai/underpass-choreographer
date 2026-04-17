//! In-memory [`AgentResolverPort`] with an explicit write surface.
//!
//! The resolver port is read-only by design — use cases never register
//! agents, they just ask for live handles. This adapter exposes an
//! additional mutation API (`insert`, `remove`) so the composition
//! root can populate the registry during startup or on the fly.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentPort, AgentRegistryPort, AgentResolverPort};
use choreo_core::value_objects::AgentId;
use tokio::sync::RwLock;

/// In-memory agent registry that doubles as an [`AgentResolverPort`].
///
/// Cloning yields a shared handle; all clones see the same state.
#[derive(Debug, Default, Clone)]
pub struct InMemoryAgentRegistry {
    inner: Arc<RwLock<BTreeMap<AgentId, Arc<dyn AgentPort>>>>,
}

impl InMemoryAgentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an agent under its [`AgentPort::id`]. Returns
    /// [`DomainError::AlreadyExists`] if that id was already taken.
    pub async fn insert(&self, agent: Arc<dyn AgentPort>) -> Result<(), DomainError> {
        let mut map = self.inner.write().await;
        let id = agent.id().clone();
        if map.contains_key(&id) {
            return Err(DomainError::AlreadyExists { what: "agent" });
        }
        map.insert(id, agent);
        Ok(())
    }

    /// Unregister an agent by id. Returns [`DomainError::NotFound`]
    /// when the id is unknown.
    pub async fn remove(&self, id: &AgentId) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .remove(id)
            .map(|_| ())
            .ok_or(DomainError::NotFound { what: "agent" })
    }

    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

#[async_trait]
impl AgentResolverPort for InMemoryAgentRegistry {
    async fn resolve(&self, id: &AgentId) -> Result<Arc<dyn AgentPort>, DomainError> {
        self.inner
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or(DomainError::NotFound { what: "agent" })
    }
}

#[async_trait]
impl AgentRegistryPort for InMemoryAgentRegistry {
    async fn register(&self, agent: Arc<dyn AgentPort>) -> Result<(), DomainError> {
        self.insert(agent).await
    }

    async fn unregister(&self, id: &AgentId) -> Result<(), DomainError> {
        self.remove(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::entities::{TaskConstraints, ValidatorReport};
    use choreo_core::ports::{Critique, DraftRequest, Revision};
    use choreo_core::value_objects::Specialty;

    #[derive(Debug)]
    struct DummyAgent {
        id: AgentId,
        specialty: Specialty,
    }
    #[async_trait]
    impl AgentPort for DummyAgent {
        fn id(&self) -> &AgentId {
            &self.id
        }
        fn specialty(&self) -> &Specialty {
            &self.specialty
        }
        async fn generate(&self, _request: DraftRequest) -> Result<Revision, DomainError> {
            Ok(Revision {
                content: String::from("x"),
            })
        }
        async fn critique(
            &self,
            _peer_content: &str,
            _constraints: &TaskConstraints,
        ) -> Result<Critique, DomainError> {
            Ok(Critique {
                feedback: String::new(),
            })
        }
        async fn revise(
            &self,
            own_content: &str,
            _critique: &Critique,
        ) -> Result<Revision, DomainError> {
            Ok(Revision {
                content: own_content.to_owned(),
            })
        }
    }

    fn agent(id: &str) -> Arc<dyn AgentPort> {
        Arc::new(DummyAgent {
            id: AgentId::new(id).unwrap(),
            specialty: Specialty::new("triage").unwrap(),
        })
    }

    // Silence the unused-field warning on ValidatorReport import above;
    // keep the import for completeness of the example.
    #[allow(dead_code)]
    fn _touches_validator_report() -> Option<ValidatorReport> {
        None
    }

    #[tokio::test]
    async fn insert_then_resolve_roundtrips() {
        let reg = InMemoryAgentRegistry::new();
        reg.insert(agent("a1")).await.unwrap();
        let got = reg.resolve(&AgentId::new("a1").unwrap()).await.unwrap();
        assert_eq!(got.id().as_str(), "a1");
    }

    #[tokio::test]
    async fn duplicate_insert_rejected() {
        let reg = InMemoryAgentRegistry::new();
        reg.insert(agent("a1")).await.unwrap();
        let err = reg.insert(agent("a1")).await.unwrap_err();
        assert!(matches!(err, DomainError::AlreadyExists { what: "agent" }));
    }

    #[tokio::test]
    async fn resolve_missing_returns_not_found() {
        let reg = InMemoryAgentRegistry::new();
        let err = reg
            .resolve(&AgentId::new("nope").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::NotFound { what: "agent" }));
    }

    #[tokio::test]
    async fn remove_deletes_entry() {
        let reg = InMemoryAgentRegistry::new();
        reg.insert(agent("a1")).await.unwrap();
        reg.remove(&AgentId::new("a1").unwrap()).await.unwrap();
        assert!(reg.is_empty().await);

        let err = reg.remove(&AgentId::new("a1").unwrap()).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { what: "agent" }));
    }

    #[tokio::test]
    async fn resolve_all_preserves_input_order() {
        let reg = InMemoryAgentRegistry::new();
        reg.insert(agent("a3")).await.unwrap();
        reg.insert(agent("a1")).await.unwrap();
        reg.insert(agent("a2")).await.unwrap();

        let ids = [
            AgentId::new("a2").unwrap(),
            AgentId::new("a1").unwrap(),
            AgentId::new("a3").unwrap(),
        ];
        let resolved = reg.resolve_all(&ids).await.unwrap();
        let order: Vec<&str> = resolved.iter().map(|a| a.id().as_str()).collect();
        assert_eq!(order, ["a2", "a1", "a3"]);
    }

    #[tokio::test]
    async fn resolve_all_fails_fast_on_missing() {
        let reg = InMemoryAgentRegistry::new();
        reg.insert(agent("a1")).await.unwrap();
        let ids = [
            AgentId::new("a1").unwrap(),
            AgentId::new("missing").unwrap(),
        ];
        let err = reg.resolve_all(&ids).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { what: "agent" }));
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let a = InMemoryAgentRegistry::new();
        let b = a.clone();
        a.insert(agent("a1")).await.unwrap();
        assert_eq!(b.len().await, 1);
    }
}
