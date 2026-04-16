//! [`CreateCouncilUseCase`] — create a council for a specialty.
//!
//! Does not materialize agents: callers pass agent ids that must
//! already be resolvable through [`AgentResolverPort`]. The
//! resolution is checked eagerly so a non-deliberating council
//! cannot be created.

use std::sync::Arc;

use choreo_core::entities::Council;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentResolverPort, ClockPort, CouncilRegistryPort};
use choreo_core::value_objects::{AgentId, CouncilId, Specialty};
use tracing::info;

#[derive(Debug, Clone)]
pub struct CreateCouncilInput {
    pub council_id: CouncilId,
    pub specialty: Specialty,
    pub agents: Vec<AgentId>,
}

pub struct CreateCouncilUseCase {
    clock: Arc<dyn ClockPort>,
    registry: Arc<dyn CouncilRegistryPort>,
    resolver: Arc<dyn AgentResolverPort>,
}

impl std::fmt::Debug for CreateCouncilUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateCouncilUseCase").finish()
    }
}

impl CreateCouncilUseCase {
    #[must_use]
    pub fn new(
        clock: Arc<dyn ClockPort>,
        registry: Arc<dyn CouncilRegistryPort>,
        resolver: Arc<dyn AgentResolverPort>,
    ) -> Self {
        Self {
            clock,
            registry,
            resolver,
        }
    }

    pub async fn execute(&self, input: CreateCouncilInput) -> Result<Council, DomainError> {
        // Eagerly check that every agent is resolvable. This fails fast
        // before inserting a council that cannot deliberate.
        let _ = self.resolver.resolve_all(&input.agents).await?;

        let council = Council::new(
            input.council_id,
            input.specialty.clone(),
            input.agents.clone(),
            self.clock.now(),
        )?;
        self.registry.register(council.clone()).await?;

        info!(
            specialty = council.specialty().as_str(),
            size = council.size(),
            "council registered"
        );
        Ok(council)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_core::ports::AgentPort;
    use time::OffsetDateTime;

    struct FrozenClock;
    impl ClockPort for FrozenClock {
        fn now(&self) -> OffsetDateTime {
            time::macros::datetime!(2026-04-15 12:00:00 UTC)
        }
    }

    struct YesResolver;
    #[async_trait]
    impl AgentResolverPort for YesResolver {
        async fn resolve(&self, _id: &AgentId) -> Result<Arc<dyn AgentPort>, DomainError> {
            // A stub "always works" resolver — returns Err only when asked
            // about unknown ids in a more sophisticated test harness.
            Err(DomainError::InvariantViolated {
                reason: "should not be called in this test",
            })
        }
        async fn resolve_all(
            &self,
            _ids: &[AgentId],
        ) -> Result<Vec<Arc<dyn AgentPort>>, DomainError> {
            Ok(vec![])
        }
    }

    struct NoResolver;
    #[async_trait]
    impl AgentResolverPort for NoResolver {
        async fn resolve(&self, _id: &AgentId) -> Result<Arc<dyn AgentPort>, DomainError> {
            Err(DomainError::NotFound { what: "agent" })
        }
        async fn resolve_all(
            &self,
            _ids: &[AgentId],
        ) -> Result<Vec<Arc<dyn AgentPort>>, DomainError> {
            Err(DomainError::NotFound { what: "agent" })
        }
    }

    #[derive(Default)]
    struct InMemoryRegistry {
        councils: std::sync::Mutex<std::collections::BTreeMap<Specialty, Council>>,
    }
    #[async_trait]
    impl CouncilRegistryPort for InMemoryRegistry {
        async fn register(&self, council: Council) -> Result<(), DomainError> {
            let mut m = self.councils.lock().unwrap();
            if m.contains_key(council.specialty()) {
                return Err(DomainError::AlreadyExists { what: "council" });
            }
            m.insert(council.specialty().clone(), council);
            Ok(())
        }
        async fn replace(&self, council: Council) -> Result<(), DomainError> {
            let mut m = self.councils.lock().unwrap();
            if !m.contains_key(council.specialty()) {
                return Err(DomainError::NotFound { what: "council" });
            }
            m.insert(council.specialty().clone(), council);
            Ok(())
        }
        async fn get(&self, specialty: &Specialty) -> Result<Council, DomainError> {
            self.councils
                .lock()
                .unwrap()
                .get(specialty)
                .cloned()
                .ok_or(DomainError::NotFound { what: "council" })
        }
        async fn list(&self) -> Result<Vec<Council>, DomainError> {
            Ok(self.councils.lock().unwrap().values().cloned().collect())
        }
        async fn delete(&self, specialty: &Specialty) -> Result<(), DomainError> {
            self.councils
                .lock()
                .unwrap()
                .remove(specialty)
                .map(|_| ())
                .ok_or(DomainError::NotFound { what: "council" })
        }
        async fn contains(&self, specialty: &Specialty) -> Result<bool, DomainError> {
            Ok(self.councils.lock().unwrap().contains_key(specialty))
        }
    }

    #[tokio::test]
    async fn registers_council_on_happy_path() {
        let usecase = CreateCouncilUseCase::new(
            Arc::new(FrozenClock),
            Arc::new(InMemoryRegistry::default()),
            Arc::new(YesResolver),
        );
        let out = usecase
            .execute(CreateCouncilInput {
                council_id: CouncilId::new("c").unwrap(),
                specialty: Specialty::new("reviewer").unwrap(),
                agents: vec![AgentId::new("a1").unwrap()],
            })
            .await
            .unwrap();

        assert_eq!(out.specialty().as_str(), "reviewer");
        assert_eq!(out.size(), 1);
    }

    #[tokio::test]
    async fn unresolvable_agent_aborts_creation() {
        let registry = Arc::new(InMemoryRegistry::default());
        let usecase = CreateCouncilUseCase::new(
            Arc::new(FrozenClock),
            registry.clone(),
            Arc::new(NoResolver),
        );
        let err = usecase
            .execute(CreateCouncilInput {
                council_id: CouncilId::new("c").unwrap(),
                specialty: Specialty::new("reviewer").unwrap(),
                agents: vec![AgentId::new("a1").unwrap()],
            })
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::NotFound { what: "agent" }));
        assert!(
            registry.councils.lock().unwrap().is_empty(),
            "registry must stay empty when resolver fails"
        );
    }

    #[tokio::test]
    async fn duplicate_specialty_is_rejected_by_registry() {
        let registry = Arc::new(InMemoryRegistry::default());
        let usecase = CreateCouncilUseCase::new(
            Arc::new(FrozenClock),
            registry.clone(),
            Arc::new(YesResolver),
        );
        let input = CreateCouncilInput {
            council_id: CouncilId::new("c").unwrap(),
            specialty: Specialty::new("x").unwrap(),
            agents: vec![AgentId::new("a").unwrap()],
        };
        usecase.execute(input.clone()).await.unwrap();
        let err = usecase.execute(input).await.unwrap_err();
        assert!(matches!(err, DomainError::AlreadyExists { .. }));
    }
}
