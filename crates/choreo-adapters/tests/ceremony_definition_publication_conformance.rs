//! Publication against its contract — and against the mistake it exists
//! to prevent.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_adapters::memory::InMemoryCeremonyDefinitionPublications;
use choreo_core::conformance::CeremonyDefinitionPublicationConformance;
use choreo_core::entities::{PublicationOutcome, PublishedCeremonyDefinition};
use choreo_core::error::DomainError;
use choreo_core::ports::CeremonyDefinitionPublicationPort;
use choreo_core::value_objects::{CeremonyName, CeremonyVersion};
use tokio::sync::RwLock;

#[tokio::test]
async fn the_in_memory_publications_satisfy_the_contract() {
    let publications = InMemoryCeremonyDefinitionPublications::new();

    let passed = CeremonyDefinitionPublicationConformance::run(&publications)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 5, "properties run: {passed:?}");
    assert!(passed.contains(&"a_taken_version_is_never_overwritten"));
}

#[tokio::test]
async fn the_suite_rejects_a_store_that_saves_over_a_published_version() {
    let publications = OverwritingPublications::default();

    let failure = CeremonyDefinitionPublicationConformance::run(&publications)
        .await
        .expect_err("a store that overwrites a published version must not pass");

    assert_eq!(failure.property(), "a_taken_version_is_never_overwritten");
}

/// A store with the semantics of the ordinary definition repository:
/// `save` accepts everything and the last writer wins.
///
/// This is not a strawman. It is what the existing
/// `CeremonyDefinitionRepositoryPort` does, and reusing it for
/// publication is the obvious shortcut — one that leaves an instance
/// bound to a version whose content changed underneath it, with nothing
/// reporting the substitution.
#[derive(Debug, Default, Clone)]
struct OverwritingPublications {
    inner: Arc<RwLock<BTreeMap<(CeremonyName, CeremonyVersion), PublishedCeremonyDefinition>>>,
}

#[async_trait]
impl CeremonyDefinitionPublicationPort for OverwritingPublications {
    async fn publish(
        &self,
        definition: PublishedCeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError> {
        let key = (definition.name().clone(), definition.version().clone());
        let mut published = self.inner.write().await;

        // Idempotency is easy to get right and this store does, which
        // is what makes it a fair counterexample: everything looks
        // correct until content actually differs.
        if let Some(occupant) = published.get(&key) {
            if occupant.digest() == definition.digest() {
                return Ok(PublicationOutcome::AlreadyPublished(occupant.clone()));
            }
        }
        published.insert(key, definition.clone());
        Ok(PublicationOutcome::Published(definition))
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
