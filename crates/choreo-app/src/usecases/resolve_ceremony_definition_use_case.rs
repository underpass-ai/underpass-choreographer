//! [`ResolveCeremonyDefinitionUseCase`] — the definition an instance
//! actually runs.
//!
//! Shared by every distribution on purpose. Resolving a binding is a
//! rule about what a working session *is*, so a server that
//! reimplemented it would be aligned with the embedded engine only
//! until one of the two changed.

use std::sync::Arc;

use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyDefinitionPublicationPort, CeremonyDefinitionRepositoryPort};

pub struct ResolveCeremonyDefinitionUseCase {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    publications: Arc<dyn CeremonyDefinitionPublicationPort>,
}

impl std::fmt::Debug for ResolveCeremonyDefinitionUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolveCeremonyDefinitionUseCase").finish()
    }
}

impl ResolveCeremonyDefinitionUseCase {
    #[must_use]
    pub fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
        publications: Arc<dyn CeremonyDefinitionPublicationPort>,
    ) -> Self {
        Self {
            definitions,
            publications,
        }
    }

    /// A bound instance is resolved from the published catalogue and
    /// **checked against the digest it recorded**. That check is the
    /// reason for storing the digest: without it, a name and a version
    /// are a promise that whatever answers to them today is what ran,
    /// and a reader has no way to tell when it is not.
    ///
    /// An unbound instance resolves from the repository, where nothing
    /// can be checked — the honest difference between the two ways of
    /// starting a working session.
    pub async fn execute(
        &self,
        instance: &CeremonyInstance,
    ) -> Result<CeremonyDefinition, DomainError> {
        let Some(digest) = instance.bound_definition() else {
            return self
                .definitions
                .get(instance.definition_name(), instance.definition_version())
                .await;
        };

        let published = self
            .publications
            .published(instance.definition_name(), instance.definition_version())
            .await?
            .ok_or(DomainError::NotFound {
                what: "published_ceremony_definition",
            })?;
        if published.digest() != digest {
            return Err(DomainError::InvariantViolated {
                reason: "the published definition no longer matches the digest this instance ran",
            });
        }
        Ok(published.into_definition())
    }
}
