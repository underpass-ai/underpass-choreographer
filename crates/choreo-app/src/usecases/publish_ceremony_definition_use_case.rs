//! [`PublishCeremonyDefinitionUseCase`] — fix a definition to an
//! immutable version.

use std::sync::Arc;

use choreo_core::entities::{CeremonyDefinition, PublicationOutcome, PublishedCeremonyDefinition};
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyDefinitionPublicationPort;

pub struct PublishCeremonyDefinitionUseCase {
    publications: Arc<dyn CeremonyDefinitionPublicationPort>,
}

impl std::fmt::Debug for PublishCeremonyDefinitionUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishCeremonyDefinitionUseCase").finish()
    }
}

impl PublishCeremonyDefinitionUseCase {
    #[must_use]
    pub fn new(publications: Arc<dyn CeremonyDefinitionPublicationPort>) -> Self {
        Self { publications }
    }

    /// A `CeremonyDefinition` cannot exist while invalid, so validation
    /// is not repeated here: whatever reaches this point already
    /// satisfies every structural rule. What publication adds is
    /// identity and immutability.
    #[tracing::instrument(
        name = "publish_ceremony_definition",
        skip_all,
        fields(ceremony = %definition.name(), version = %definition.version())
    )]
    pub async fn execute(
        &self,
        definition: CeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError> {
        let sealed = PublishedCeremonyDefinition::seal(definition)?;
        self.publications.publish(sealed).await
    }
}
