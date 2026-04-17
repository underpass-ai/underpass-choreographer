//! [`ListCouncilsUseCase`] — enumerate all registered councils.

use std::sync::Arc;

use choreo_core::entities::Council;
use choreo_core::error::DomainError;
use choreo_core::ports::CouncilRegistryPort;

pub struct ListCouncilsUseCase {
    registry: Arc<dyn CouncilRegistryPort>,
}

impl std::fmt::Debug for ListCouncilsUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListCouncilsUseCase").finish()
    }
}

impl ListCouncilsUseCase {
    #[must_use]
    pub fn new(registry: Arc<dyn CouncilRegistryPort>) -> Self {
        Self { registry }
    }

    #[tracing::instrument(name = "list_councils", skip_all)]
    pub async fn execute(&self) -> Result<Vec<Council>, DomainError> {
        self.registry.list().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_core::value_objects::{AgentId, CouncilId, Specialty};
    use time::macros::datetime;

    struct FixedList {
        councils: Vec<Council>,
    }
    #[async_trait]
    impl CouncilRegistryPort for FixedList {
        async fn register(&self, _council: Council) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn replace(&self, _council: Council) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn get(&self, _specialty: &Specialty) -> Result<Council, DomainError> {
            unimplemented!()
        }
        async fn list(&self) -> Result<Vec<Council>, DomainError> {
            Ok(self.councils.clone())
        }
        async fn delete(&self, _specialty: &Specialty) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn contains(&self, _specialty: &Specialty) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn returns_everything_from_registry() {
        let councils = vec![
            Council::new(
                CouncilId::new("c1").unwrap(),
                Specialty::new("triage").unwrap(),
                vec![AgentId::new("a").unwrap()],
                datetime!(2026-04-15 12:00:00 UTC),
            )
            .unwrap(),
            Council::new(
                CouncilId::new("c2").unwrap(),
                Specialty::new("reviewer").unwrap(),
                vec![AgentId::new("a").unwrap()],
                datetime!(2026-04-15 12:00:00 UTC),
            )
            .unwrap(),
        ];
        let usecase = ListCouncilsUseCase::new(Arc::new(FixedList { councils }));
        let out = usecase.execute().await.unwrap();
        assert_eq!(out.len(), 2);
    }
}
