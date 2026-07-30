//! In-memory [`CeremonyDefinitionPublicationPort`] implementation.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::{PublicationOutcome, PublishedCeremonyDefinition};
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyDefinitionPublicationPort;
use choreo_core::value_objects::{CeremonyName, CeremonyVersion};
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct InMemoryCeremonyDefinitionPublications {
    inner: Arc<RwLock<BTreeMap<(CeremonyName, CeremonyVersion), PublishedCeremonyDefinition>>>,
}

impl InMemoryCeremonyDefinitionPublications {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CeremonyDefinitionPublicationPort for InMemoryCeremonyDefinitionPublications {
    /// The occupant is read and the slot written under one write lock:
    /// releasing it in between would let two callers publish different
    /// content under one version.
    async fn publish(
        &self,
        definition: PublishedCeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError> {
        let key = (definition.name().clone(), definition.version().clone());
        let mut published = self.inner.write().await;

        Ok(match published.get(&key) {
            Some(occupant) if occupant.digest() == definition.digest() => {
                PublicationOutcome::AlreadyPublished(occupant.clone())
            }
            Some(occupant) => PublicationOutcome::VersionOccupied {
                published: occupant.digest(),
                offered: definition.digest(),
            },
            None => {
                published.insert(key, definition.clone());
                PublicationOutcome::Published(definition)
            }
        })
    }

    async fn published(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<Option<PublishedCeremonyDefinition>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(&(name.clone(), version.clone()))
            .cloned())
    }

    async fn catalogue(&self) -> Result<Vec<PublishedCeremonyDefinition>, DomainError> {
        Ok(self.inner.read().await.values().cloned().collect())
    }
}
