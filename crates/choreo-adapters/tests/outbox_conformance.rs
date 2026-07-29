//! The in-memory store against the outbox contract, and a store that
//! hands out a whole ceremony at once so the ordering guarantee is
//! shown to be checked rather than described.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use choreo_adapters::memory::InMemoryCeremonyStore;
use choreo_core::conformance::OutboxConformance;
use choreo_core::entities::{CeremonyCommit, CommitOutcome};
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyUnitOfWorkPort, OutboxPort};
use choreo_core::value_objects::{
    CeremonyId, CeremonyRevision, ClaimedOutboxMessage, DurationMs, EventId, OutboxAttempt,
    OutboxMessage, OutboxQuarantineReason,
};
use time::OffsetDateTime;
use tokio::sync::RwLock;

#[tokio::test]
async fn the_in_memory_store_satisfies_the_outbox_contract() {
    let store = InMemoryCeremonyStore::new();

    let passed = OutboxConformance::run(&store, &store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 7, "properties run: {passed:?}");
    assert!(passed.contains(&"a_claim_yields_at_most_one_message_per_ceremony"));
    assert!(passed.contains(&"a_quarantined_message_blocks_only_its_own_ceremony"));
    assert!(passed.contains(&"an_expired_claim_becomes_claimable"));
}

#[tokio::test]
async fn the_suite_rejects_a_store_with_one_global_queue() {
    let store = FlatQueueStore::default();

    let failure = OutboxConformance::run(&store, &store)
        .await
        .expect_err("a store that ignores ceremony boundaries must not pass");

    assert_eq!(
        failure.property(),
        "a_claim_yields_at_most_one_message_per_ceremony"
    );
}

/// A store that keeps one queue for everything instead of one per
/// ceremony.
///
/// It is the natural first implementation, and it is wrong in a way
/// nothing else catches: every message is delivered, none is lost, and
/// a single ceremony's events can still arrive out of order because a
/// publisher was handed two of them at once.
#[derive(Debug, Default, Clone)]
struct FlatQueueStore {
    inner: Arc<RwLock<FlatQueue>>,
}

#[derive(Debug, Default)]
struct FlatQueue {
    revisions: BTreeMap<CeremonyId, CeremonyRevision>,
    messages: Vec<FlatEntry>,
}

#[derive(Debug, Clone)]
struct FlatEntry {
    message: OutboxMessage,
    attempt: OutboxAttempt,
    claimed_until: Option<OffsetDateTime>,
    delivered: bool,
    quarantined: bool,
}

#[async_trait]
impl CeremonyUnitOfWorkPort for FlatQueueStore {
    async fn commit(&self, commit: CeremonyCommit) -> Result<CommitOutcome, DomainError> {
        let mut queue = self.inner.write().await;
        let ceremony_id = commit.instance().id().clone();
        let stored = queue.revisions.get(&ceremony_id).copied();
        let (_, expected, _, messages) = commit.into_parts();
        if !expected.matches(stored) {
            return Ok(CommitOutcome::Conflict { expected, stored });
        }
        let revision = expected.resulting_revision();
        queue.revisions.insert(ceremony_id, revision);
        queue
            .messages
            .extend(messages.into_iter().map(|message| FlatEntry {
                message,
                attempt: OutboxAttempt::NONE,
                claimed_until: None,
                delivered: false,
                quarantined: false,
            }));
        Ok(CommitOutcome::Committed {
            revision,
            records: Vec::new(),
        })
    }

    async fn revision(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Result<Option<CeremonyRevision>, DomainError> {
        Ok(self.inner.read().await.revisions.get(ceremony_id).copied())
    }
}

#[async_trait]
impl OutboxPort for FlatQueueStore {
    async fn claim(
        &self,
        limit: usize,
        now: OffsetDateTime,
        lease: DurationMs,
    ) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
        let lease_until = now + Duration::from_millis(lease.get());
        let mut queue = self.inner.write().await;
        let mut claimed = Vec::new();
        for entry in &mut queue.messages {
            if claimed.len() >= limit {
                break;
            }
            if entry.delivered
                || entry.quarantined
                || entry.claimed_until.is_some_and(|until| until > now)
            {
                continue;
            }
            entry.claimed_until = Some(lease_until);
            claimed.push(ClaimedOutboxMessage::new(
                entry.message.clone(),
                entry.attempt,
            ));
        }
        Ok(claimed)
    }

    async fn mark_delivered(&self, event_ids: &[EventId]) -> Result<(), DomainError> {
        let mut queue = self.inner.write().await;
        for entry in &mut queue.messages {
            if event_ids.contains(entry.message.event_id()) {
                entry.delivered = true;
                entry.claimed_until = None;
            }
        }
        Ok(())
    }

    async fn mark_failed(&self, event_id: &EventId) -> Result<(), DomainError> {
        let mut queue = self.inner.write().await;
        for entry in &mut queue.messages {
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
        _reason: OutboxQuarantineReason,
    ) -> Result<(), DomainError> {
        let mut queue = self.inner.write().await;
        for entry in &mut queue.messages {
            if entry.message.event_id() == event_id {
                entry.quarantined = true;
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
            .messages
            .iter()
            .filter(|entry| entry.quarantined)
            .map(|entry| ClaimedOutboxMessage::new(entry.message.clone(), entry.attempt))
            .collect())
    }
}
