//! [`GetDeliberationUseCase`] — fetch a previously-saved deliberation.

use std::sync::Arc;

use choreo_core::entities::Deliberation;
use choreo_core::error::DomainError;
use choreo_core::ports::DeliberationRepositoryPort;
use choreo_core::value_objects::TaskId;

pub struct GetDeliberationUseCase {
    repository: Arc<dyn DeliberationRepositoryPort>,
}

impl std::fmt::Debug for GetDeliberationUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetDeliberationUseCase").finish()
    }
}

impl GetDeliberationUseCase {
    #[must_use]
    pub fn new(repository: Arc<dyn DeliberationRepositoryPort>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(
        name = "get_deliberation",
        skip_all,
        fields(task_id = %task_id)
    )]
    pub async fn execute(&self, task_id: &TaskId) -> Result<Deliberation, DomainError> {
        self.repository.get(task_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MissingRepo;
    #[async_trait]
    impl DeliberationRepositoryPort for MissingRepo {
        async fn save(&self, _deliberation: &Deliberation) -> Result<(), DomainError> {
            Ok(())
        }
        async fn get(&self, _task_id: &TaskId) -> Result<Deliberation, DomainError> {
            Err(DomainError::NotFound {
                what: "deliberation",
            })
        }
        async fn exists(&self, _task_id: &TaskId) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn missing_deliberation_returns_not_found() {
        let usecase = GetDeliberationUseCase::new(Arc::new(MissingRepo));
        let err = usecase
            .execute(&TaskId::new("t").unwrap())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::NotFound {
                what: "deliberation"
            }
        ));
    }
}
