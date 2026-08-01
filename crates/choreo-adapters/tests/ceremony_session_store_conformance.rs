//! The in-memory ceremony store against the one-storage contract — and
//! a pair of adapters wired the way the composition roots used to wire
//! them, so the suite is shown to catch the mistake it exists for. The
//! durable store meets the same suite in its own file.

use std::sync::Arc;

use async_trait::async_trait;
use choreo_adapters::memory::{InMemoryCeremonyInstanceRepository, InMemoryCeremonyStore};
use choreo_core::conformance::CeremonySessionStoreConformance;
use choreo_core::entities::{CeremonyCommit, CeremonyInstance, CommitOutcome};
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyInstanceRepositoryPort, CeremonyUnitOfWorkPort};
use choreo_core::value_objects::{CeremonyId, CeremonyRevision};

#[tokio::test]
async fn the_in_memory_store_serves_both_ports_over_one_storage() {
    let store = InMemoryCeremonyStore::new();

    let passed = CeremonySessionStoreConformance::run(&store, &store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 3, "properties run: {passed:?}");
}

/// The wiring this suite was written for.
///
/// Two perfectly good adapters, each passing its own contract, composed
/// so that sessions are committed into one storage and read out of
/// another. Every call succeeds. This is not a hypothetical: it is what
/// the composition roots built until the session journal needed both
/// ports at once and the halves stopped agreeing.
#[tokio::test]
async fn the_suite_rejects_two_storages_pretending_to_be_one() {
    let store = TwoStoragesPretendingToBeOne::default();

    let failure = CeremonySessionStoreConformance::run(&store, &store)
        .await
        .expect_err("two storages must not pass a suite about one");

    assert_eq!(failure.property(), "a_committed_session_can_be_read_back");
    assert!(
        failure.detail().contains("not over one storage"),
        "the failure must name the cause: {}",
        failure.detail()
    );
}

#[derive(Default)]
struct TwoStoragesPretendingToBeOne {
    reads: Arc<InMemoryCeremonyInstanceRepository>,
    writes: Arc<InMemoryCeremonyStore>,
}

#[async_trait]
impl CeremonyInstanceRepositoryPort for TwoStoragesPretendingToBeOne {
    async fn save(&self, instance: &CeremonyInstance) -> Result<(), DomainError> {
        self.reads.save(instance).await
    }

    async fn get(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError> {
        self.reads.get(id).await
    }

    async fn list(&self) -> Result<Vec<CeremonyInstance>, DomainError> {
        self.reads.list().await
    }

    async fn exists(&self, id: &CeremonyId) -> Result<bool, DomainError> {
        self.reads.exists(id).await
    }
}

#[async_trait]
impl CeremonyUnitOfWorkPort for TwoStoragesPretendingToBeOne {
    async fn commit(&self, commit: CeremonyCommit) -> Result<CommitOutcome, DomainError> {
        self.writes.commit(commit).await
    }

    async fn revision(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Result<Option<CeremonyRevision>, DomainError> {
        self.writes.revision(ceremony_id).await
    }
}
