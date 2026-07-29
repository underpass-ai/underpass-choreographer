//! In-memory store implementing the transactional boundary.
//!
//! State, journal and outbox live under one lock, because that is the
//! whole claim: a commit that touches all three either lands or does
//! not. Two collaborating adapters with a lock each would satisfy every
//! property except the one that matters.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::{
    AuditFact, AuditRecord, CeremonyCommit, CeremonyInstance, CommitOutcome,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{AuditJournalPort, CeremonyUnitOfWorkPort};
use choreo_core::value_objects::{CeremonyId, CeremonyRevision, OutboxMessage};
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
struct StoredCeremony {
    revision: Option<CeremonyRevision>,
    instance: Option<CeremonyInstance>,
    journal: Vec<AuditRecord>,
    outbox: Vec<OutboxMessage>,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryCeremonyStore {
    inner: Arc<RwLock<BTreeMap<CeremonyId, StoredCeremony>>>,
}

impl InMemoryCeremonyStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn instance(&self, ceremony_id: &CeremonyId) -> Option<CeremonyInstance> {
        self.inner
            .read()
            .await
            .get(ceremony_id)
            .and_then(|stored| stored.instance.clone())
    }

    /// Messages awaiting publication, in the order they were enqueued.
    pub async fn outbox(&self, ceremony_id: &CeremonyId) -> Vec<OutboxMessage> {
        self.inner
            .read()
            .await
            .get(ceremony_id)
            .map(|stored| stored.outbox.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl CeremonyUnitOfWorkPort for InMemoryCeremonyStore {
    async fn commit(&self, commit: CeremonyCommit) -> Result<CommitOutcome, DomainError> {
        let mut ceremonies = self.inner.write().await;
        let stored = ceremonies
            .entry(commit.instance().id().clone())
            .or_default();

        let (instance, expected, facts, messages) = commit.into_parts();
        if !expected.matches(stored.revision) {
            return Ok(CommitOutcome::Conflict {
                expected,
                stored: stored.revision,
            });
        }

        // Seal against a copy of the journal so a rejected fact leaves
        // the stored one untouched: nothing is written until every
        // record is known to be sound.
        let mut journal = stored.journal.clone();
        let mut sealed = Vec::with_capacity(facts.len());
        for fact in facts {
            let record = match journal.last() {
                Some(head) => AuditRecord::following(fact, head)?,
                None => AuditRecord::first(fact)?,
            };
            journal.push(record.clone());
            sealed.push(record);
        }

        let revision = expected.resulting_revision();
        stored.revision = Some(revision);
        stored.instance = Some(instance);
        stored.journal = journal;
        stored.outbox.extend(messages);

        Ok(CommitOutcome::Committed {
            revision,
            records: sealed,
        })
    }

    async fn revision(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Result<Option<CeremonyRevision>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(ceremony_id)
            .and_then(|stored| stored.revision))
    }
}

#[async_trait]
impl AuditJournalPort for InMemoryCeremonyStore {
    async fn append(&self, fact: AuditFact) -> Result<AuditRecord, DomainError> {
        let mut ceremonies = self.inner.write().await;
        let stored = ceremonies.entry(fact.ceremony_id.clone()).or_default();
        let record = match stored.journal.last() {
            Some(head) => AuditRecord::following(fact, head)?,
            None => AuditRecord::first(fact)?,
        };
        stored.journal.push(record.clone());
        Ok(record)
    }

    async fn head(&self, ceremony_id: &CeremonyId) -> Result<Option<AuditRecord>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(ceremony_id)
            .and_then(|stored| stored.journal.last())
            .cloned())
    }

    async fn records(&self, ceremony_id: &CeremonyId) -> Result<Vec<AuditRecord>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(ceremony_id)
            .map(|stored| stored.journal.clone())
            .unwrap_or_default())
    }
}
