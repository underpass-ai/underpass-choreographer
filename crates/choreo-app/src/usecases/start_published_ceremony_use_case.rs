//! [`StartPublishedCeremonyUseCase`] — run a published definition, and
//! record which one.

use std::sync::Arc;

use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionPublicationPort, CeremonyInstanceRepositoryPort, ClockPort,
};

use super::start_ceremony_input::StartCeremonyInput;

pub struct StartPublishedCeremonyUseCase {
    publications: Arc<dyn CeremonyDefinitionPublicationPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for StartPublishedCeremonyUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartPublishedCeremonyUseCase").finish()
    }
}

impl StartPublishedCeremonyUseCase {
    #[must_use]
    pub fn new(
        publications: Arc<dyn CeremonyDefinitionPublicationPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            publications,
            instances,
            clock,
        }
    }

    /// Resolve the published version and bind the instance to its
    /// digest.
    ///
    /// Deliberately not a fallback to an unpublished definition of the
    /// same name: a caller that asked for a published version and
    /// silently received something else would be told it is governed
    /// when it is not.
    #[tracing::instrument(
        name = "start_published_ceremony",
        skip_all,
        fields(ceremony_id = %input.id)
    )]
    pub async fn execute(
        &self,
        input: StartCeremonyInput,
    ) -> Result<CeremonyInstance, DomainError> {
        if self.instances.exists(&input.id).await? {
            return Err(DomainError::AlreadyExists {
                what: "ceremony_instance",
            });
        }

        let published = self
            .publications
            .published(&input.definition_name, &input.definition_version)
            .await?
            .ok_or(DomainError::NotFound {
                what: "published_ceremony_definition",
            })?;

        let instance =
            CeremonyInstance::start_bound(input.id, &published, input.context, self.clock.now());
        self.instances.save(&instance).await?;
        Ok(instance)
    }
}
