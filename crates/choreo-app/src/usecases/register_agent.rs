//! [`RegisterAgentUseCase`] — materialize an agent from a descriptor
//! via [`AgentFactoryPort`] and store it in the registry through
//! [`AgentRegistryPort`].
//!
//! This use case does **not** attach the agent to any council. Council
//! membership stays under the responsibility of
//! [`crate::usecases::CreateCouncilUseCase`]; registering the agent
//! only makes it resolvable by id — subsequent calls to
//! `CreateCouncil` or `Deliberate` may then include it.
//!
//! Returns the [`AgentId`] that was registered so gRPC handlers can
//! echo it back in the response.

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::{AgentDescriptor, AgentFactoryPort, AgentRegistryPort};
use choreo_core::value_objects::AgentId;
use tracing::info;

pub struct RegisterAgentUseCase {
    factory: Arc<dyn AgentFactoryPort>,
    registry: Arc<dyn AgentRegistryPort>,
}

impl std::fmt::Debug for RegisterAgentUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisterAgentUseCase").finish()
    }
}

impl RegisterAgentUseCase {
    #[must_use]
    pub fn new(factory: Arc<dyn AgentFactoryPort>, registry: Arc<dyn AgentRegistryPort>) -> Self {
        Self { factory, registry }
    }

    #[tracing::instrument(
        name = "register_agent",
        skip_all,
        fields(
            agent_id = %descriptor.id,
            specialty = %descriptor.specialty,
            kind = %descriptor.kind,
        )
    )]
    pub async fn execute(&self, descriptor: AgentDescriptor) -> Result<AgentId, DomainError> {
        let id = descriptor.id.clone();
        let kind = descriptor.kind.clone();
        let specialty = descriptor.specialty.clone();

        let agent = self.factory.create(descriptor).await?;
        self.registry.register(agent).await?;

        info!(
            agent_id = id.as_str(),
            specialty = specialty.as_str(),
            kind = kind.as_str(),
            "agent registered"
        );
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_core::entities::TaskConstraints;
    use choreo_core::ports::{AgentPort, Critique, DraftRequest, Revision};
    use choreo_core::value_objects::{AgentKind, Attributes, Specialty};
    use std::sync::Mutex;

    #[derive(Debug)]
    struct StubAgent {
        id: AgentId,
        specialty: Specialty,
    }
    #[async_trait]
    impl AgentPort for StubAgent {
        fn id(&self) -> &AgentId {
            &self.id
        }
        fn specialty(&self) -> &Specialty {
            &self.specialty
        }
        async fn generate(&self, _request: DraftRequest) -> Result<Revision, DomainError> {
            Ok(Revision {
                content: String::new(),
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
        async fn revise(&self, own: &str, _critique: &Critique) -> Result<Revision, DomainError> {
            Ok(Revision {
                content: own.to_owned(),
            })
        }
    }

    #[derive(Default)]
    struct RecordingFactory {
        kind_accepted: &'static str,
        created: Mutex<Vec<AgentId>>,
    }
    #[async_trait]
    impl AgentFactoryPort for RecordingFactory {
        async fn create(
            &self,
            descriptor: AgentDescriptor,
        ) -> Result<Arc<dyn AgentPort>, DomainError> {
            if descriptor.kind.as_str() != self.kind_accepted {
                return Err(DomainError::InvariantViolated {
                    reason: "test factory: unsupported kind",
                });
            }
            self.created.lock().unwrap().push(descriptor.id.clone());
            Ok(Arc::new(StubAgent {
                id: descriptor.id,
                specialty: descriptor.specialty,
            }))
        }
    }

    #[derive(Default)]
    struct InMemoryRegistry {
        inserted: Mutex<Vec<AgentId>>,
        reject_second: bool,
    }
    #[async_trait]
    impl AgentRegistryPort for InMemoryRegistry {
        async fn register(&self, agent: Arc<dyn AgentPort>) -> Result<(), DomainError> {
            let mut v = self.inserted.lock().unwrap();
            if self.reject_second && !v.is_empty() {
                return Err(DomainError::AlreadyExists { what: "agent" });
            }
            v.push(agent.id().clone());
            Ok(())
        }
        async fn unregister(&self, _id: &AgentId) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    fn descriptor(id: &str, kind: &str) -> AgentDescriptor {
        AgentDescriptor {
            id: AgentId::new(id).unwrap(),
            specialty: Specialty::new("triage").unwrap(),
            kind: AgentKind::new(kind).unwrap(),
            attributes: Attributes::empty(),
        }
    }

    #[tokio::test]
    async fn happy_path_creates_and_registers() {
        let factory = Arc::new(RecordingFactory {
            kind_accepted: "noop",
            ..RecordingFactory::default()
        });
        let registry = Arc::new(InMemoryRegistry::default());
        let usecase = RegisterAgentUseCase::new(factory.clone(), registry.clone());

        let id = usecase.execute(descriptor("a1", "noop")).await.unwrap();
        assert_eq!(id.as_str(), "a1");
        assert_eq!(factory.created.lock().unwrap().len(), 1);
        assert_eq!(registry.inserted.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn factory_rejection_propagates_unchanged() {
        let factory = Arc::new(RecordingFactory {
            kind_accepted: "noop",
            ..RecordingFactory::default()
        });
        let registry = Arc::new(InMemoryRegistry::default());
        let usecase = RegisterAgentUseCase::new(factory, registry.clone());

        let err = usecase.execute(descriptor("a1", "vllm")).await.unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
        // Registry must stay empty when factory rejects.
        assert!(registry.inserted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn registry_rejection_does_not_eat_created_agent() {
        let factory = Arc::new(RecordingFactory {
            kind_accepted: "noop",
            ..RecordingFactory::default()
        });
        let registry = Arc::new(InMemoryRegistry {
            reject_second: true,
            ..InMemoryRegistry::default()
        });
        let usecase = RegisterAgentUseCase::new(factory.clone(), registry.clone());

        usecase.execute(descriptor("a1", "noop")).await.unwrap();
        let err = usecase.execute(descriptor("a2", "noop")).await.unwrap_err();
        assert!(matches!(err, DomainError::AlreadyExists { what: "agent" }));
        // Factory was invoked twice (even though only one made it to
        // the registry). This is by design: the factory owns the
        // agent's lifetime and can clean up its side effects if any.
        assert_eq!(factory.created.lock().unwrap().len(), 2);
    }
}
