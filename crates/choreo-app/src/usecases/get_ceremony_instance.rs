use std::fmt;
use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyInstanceRepositoryPort;
use choreo_core::value_objects::CeremonyId;

/// Retrieves a ceremony instance without exposing its persistence adapter.
pub struct GetCeremonyInstanceUseCase {
    repository: Arc<dyn CeremonyInstanceRepositoryPort>,
}

impl fmt::Debug for GetCeremonyInstanceUseCase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GetCeremonyInstanceUseCase")
            .finish()
    }
}

impl GetCeremonyInstanceUseCase {
    #[must_use]
    pub fn new(repository: Arc<dyn CeremonyInstanceRepositoryPort>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(name = "get_ceremony_instance", skip_all, fields(ceremony_id = %id))]
    pub async fn execute(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError> {
        self.repository.get(id).await
    }
}
