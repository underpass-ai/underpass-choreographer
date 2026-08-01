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
use choreo_core::ports::{
    AuditJournalPort, CeremonyInstanceRepositoryPort, CeremonyUnitOfWorkPort, OutboxPort,
};
use choreo_core::value_objects::{
    CeremonyId, CeremonyRevision, ClaimedOutboxMessage, DurationMs, EventId, OutboxAttempt,
    OutboxMessage, OutboxQuarantineReason,
};
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
struct StoredCeremony {
    revision: Option<CeremonyRevision>,
    instance: Option<CeremonyInstance>,
    journal: Vec<AuditRecord>,
    outbox: Vec<StoredOutboxMessage>,
}

/// A committed message and everything the store knows about getting it
/// out. Delivery state lives here rather than on the message for the
/// same reason the revision does: it describes the store's dealings
/// with the message, not what the ceremony did.
#[derive(Debug, Clone)]
struct StoredOutboxMessage {
    message: OutboxMessage,
    attempt: OutboxAttempt,
    claimed_until: Option<OffsetDateTime>,
    delivered: bool,
    quarantine: Option<OutboxQuarantineReason>,
}

impl StoredOutboxMessage {
    fn new(message: OutboxMessage) -> Self {
        Self {
            message,
            attempt: OutboxAttempt::NONE,
            claimed_until: None,
            delivered: false,
            quarantine: None,
        }
    }

    fn is_claimable(&self, now: OffsetDateTime) -> bool {
        !self.delivered
            && self.quarantine.is_none()
            && self.claimed_until.is_none_or(|until| until <= now)
    }
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
            .map(|stored| {
                stored
                    .outbox
                    .iter()
                    .filter(|entry| !entry.delivered)
                    .map(|entry| entry.message.clone())
                    .collect()
            })
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
        stored
            .outbox
            .extend(messages.into_iter().map(StoredOutboxMessage::new));

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

#[async_trait]
impl OutboxPort for InMemoryCeremonyStore {
    /// At most one message per ceremony, so a ceremony's stream cannot
    /// be reordered by a publisher handling two of its messages at
    /// once. A quarantined or already-claimed head stops that ceremony
    /// and leaves every other one alone.
    async fn claim(
        &self,
        limit: usize,
        now: OffsetDateTime,
        lease: DurationMs,
    ) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
        let lease_until = now + Duration::from_millis(lease.get());
        let mut claimed = Vec::new();

        for stored in self.inner.write().await.values_mut() {
            if claimed.len() >= limit {
                break;
            }
            let Some(head) = stored.outbox.iter_mut().find(|entry| !entry.delivered) else {
                continue;
            };
            if !head.is_claimable(now) {
                continue;
            }
            head.claimed_until = Some(lease_until);
            claimed.push(ClaimedOutboxMessage::new(
                head.message.clone(),
                head.attempt,
            ));
        }

        Ok(claimed)
    }

    async fn mark_delivered(&self, event_ids: &[EventId]) -> Result<(), DomainError> {
        let mut ceremonies = self.inner.write().await;
        for entry in ceremonies
            .values_mut()
            .flat_map(|stored| stored.outbox.iter_mut())
        {
            if event_ids.contains(entry.message.event_id()) {
                entry.delivered = true;
                entry.claimed_until = None;
            }
        }
        Ok(())
    }

    async fn mark_failed(&self, event_id: &EventId) -> Result<(), DomainError> {
        let mut ceremonies = self.inner.write().await;
        for entry in ceremonies
            .values_mut()
            .flat_map(|stored| stored.outbox.iter_mut())
        {
            if entry.message.event_id() == event_id {
                entry.attempt = entry.attempt.next();
                entry.claimed_until = None;
            }
        }
        Ok(())
    }

    async fn quarantine(
        &self,
        event_id: &EventId,
        reason: OutboxQuarantineReason,
    ) -> Result<(), DomainError> {
        let mut ceremonies = self.inner.write().await;
        for entry in ceremonies
            .values_mut()
            .flat_map(|stored| stored.outbox.iter_mut())
        {
            if entry.message.event_id() == event_id {
                entry.quarantine = Some(reason.clone());
                entry.claimed_until = None;
            }
        }
        Ok(())
    }

    async fn quarantined(&self) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .values()
            .flat_map(|stored| stored.outbox.iter())
            .filter(|entry| entry.quarantine.is_some())
            .map(|entry| ClaimedOutboxMessage::new(entry.message.clone(), entry.attempt))
            .collect())
    }
}

/// Reading and writing sessions outside a unit of work.
///
/// The same storage the transactional path uses, on purpose. Splitting
/// them across two adapters is what this module's opening paragraph
/// warns about: a session committed through one would be invisible to
/// the other, and every property would hold except the one that
/// matters.
///
/// The revision advances on a plain save even though nothing checks it
/// here, exactly as the durable store does — so a concurrent commit
/// holding a stale expectation conflicts as it should, and the weaker
/// path cannot quietly defeat the stronger one.
#[async_trait]
impl CeremonyInstanceRepositoryPort for InMemoryCeremonyStore {
    async fn save(&self, instance: &CeremonyInstance) -> Result<(), DomainError> {
        let mut ceremonies = self.inner.write().await;
        let stored = ceremonies.entry(instance.id().clone()).or_default();
        stored.revision = Some(
            stored
                .revision
                .map_or(CeremonyRevision::INITIAL, CeremonyRevision::next),
        );
        stored.instance = Some(instance.clone());
        Ok(())
    }

    async fn get(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError> {
        self.inner
            .read()
            .await
            .get(id)
            .and_then(|stored| stored.instance.clone())
            .ok_or(DomainError::NotFound {
                what: "ceremony_instance",
            })
    }

    async fn list(&self) -> Result<Vec<CeremonyInstance>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .values()
            .filter_map(|stored| stored.instance.clone())
            .collect())
    }

    async fn exists(&self, id: &CeremonyId) -> Result<bool, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(id)
            .is_some_and(|stored| stored.instance.is_some()))
    }
}
