use std::fmt;
use std::sync::Arc;

use choreo_core::entities::CeremonyDefinition;
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyDefinitionRepositoryPort;

/// Lists every mounted ceremony definition.
pub struct ListCeremonyDefinitionsUseCase {
    repository: Arc<dyn CeremonyDefinitionRepositoryPort>,
}

impl fmt::Debug for ListCeremonyDefinitionsUseCase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ListCeremonyDefinitionsUseCase")
            .finish()
    }
}

impl ListCeremonyDefinitionsUseCase {
    #[must_use]
    pub fn new(repository: Arc<dyn CeremonyDefinitionRepositoryPort>) -> Self {
        Self { repository }
    }

    #[tracing::instrument(name = "list_ceremony_definitions", skip_all)]
    pub async fn execute(&self) -> Result<Vec<CeremonyDefinition>, DomainError> {
        self.repository.list().await
    }
}
