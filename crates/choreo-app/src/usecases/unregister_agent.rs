//! [`UnregisterAgentUseCase`] — remove an agent from the registry by
//! id.
//!
//! Does **not** scrub councils that reference the id. Subsequent
//! deliberations that include the id will fail with
//! [`DomainError::NotFound`] at resolve time — which is louder than
//! silently editing council membership, and matches the semantics of
//! `DeleteCouncil` (it does not unregister member agents either).

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::AgentRegistryPort;
use choreo_core::value_objects::AgentId;
use tracing::info;

pub struct UnregisterAgentUseCase {
    registry: Arc<dyn AgentRegistryPort>,
}

impl std::fmt::Debug for UnregisterAgentUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnregisterAgentUseCase").finish()
    }
}

impl UnregisterAgentUseCase {
    #[must_use]
    pub fn new(registry: Arc<dyn AgentRegistryPort>) -> Self {
        Self { registry }
    }

    #[tracing::instrument(
        name = "unregister_agent",
        skip_all,
        fields(agent_id = %id)
    )]
    pub async fn execute(&self, id: &AgentId) -> Result<(), DomainError> {
        self.registry.unregister(id).await?;
        info!(agent_id = id.as_str(), "agent unregistered");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_core::ports::AgentPort;
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubRegistry {
        removed: Mutex<Vec<AgentId>>,
        miss: bool,
    }
    #[async_trait]
    impl AgentRegistryPort for StubRegistry {
        async fn register(&self, _agent: Arc<dyn AgentPort>) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn unregister(&self, id: &AgentId) -> Result<(), DomainError> {
            if self.miss {
                return Err(DomainError::NotFound { what: "agent" });
            }
            self.removed.lock().unwrap().push(id.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn happy_path_unregisters() {
        let registry = Arc::new(StubRegistry::default());
        let usecase = UnregisterAgentUseCase::new(registry.clone());
        usecase.execute(&AgentId::new("a1").unwrap()).await.unwrap();
        assert_eq!(
            registry.removed.lock().unwrap()[0].as_str(),
            "a1",
            "registry must receive the exact id"
        );
    }

    #[tokio::test]
    async fn missing_agent_surfaces_as_not_found() {
        let registry = Arc::new(StubRegistry {
            miss: true,
            ..StubRegistry::default()
        });
        let usecase = UnregisterAgentUseCase::new(registry);
        let err = usecase
            .execute(&AgentId::new("missing").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::NotFound { what: "agent" }));
    }
}
