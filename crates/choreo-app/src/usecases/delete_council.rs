//! [`DeleteCouncilUseCase`] — remove the council for a specialty.

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::CouncilRegistryPort;
use choreo_core::value_objects::Specialty;
use tracing::info;

pub struct DeleteCouncilUseCase {
    registry: Arc<dyn CouncilRegistryPort>,
}

impl std::fmt::Debug for DeleteCouncilUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeleteCouncilUseCase").finish()
    }
}

impl DeleteCouncilUseCase {
    #[must_use]
    pub fn new(registry: Arc<dyn CouncilRegistryPort>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, specialty: &Specialty) -> Result<(), DomainError> {
        self.registry.delete(specialty).await?;
        info!(specialty = specialty.as_str(), "council removed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_core::entities::Council;

    struct FailingRegistry;
    #[async_trait]
    impl CouncilRegistryPort for FailingRegistry {
        async fn register(&self, _council: Council) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn replace(&self, _council: Council) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn get(&self, _specialty: &Specialty) -> Result<Council, DomainError> {
            Err(DomainError::NotFound { what: "council" })
        }
        async fn list(&self) -> Result<Vec<Council>, DomainError> {
            Ok(vec![])
        }
        async fn delete(&self, _specialty: &Specialty) -> Result<(), DomainError> {
            Err(DomainError::NotFound { what: "council" })
        }
        async fn contains(&self, _specialty: &Specialty) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn missing_council_propagates_not_found() {
        let usecase = DeleteCouncilUseCase::new(Arc::new(FailingRegistry));
        let err = usecase
            .execute(&Specialty::new("none").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::NotFound { what: "council" }));
    }
}
