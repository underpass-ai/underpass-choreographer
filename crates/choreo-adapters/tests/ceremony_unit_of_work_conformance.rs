//! The in-memory ceremony store against the transactional contract —
//! and a store that commits its three parts one after another, so the
//! suite is shown to catch the mistake it exists for.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_adapters::memory::InMemoryCeremonyStore;
use choreo_core::conformance::CeremonyUnitOfWorkConformance;
use choreo_core::entities::{
    AuditFact, AuditRecord, CeremonyCommit, CeremonyInstance, CommitOutcome,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{AuditJournalPort, CeremonyUnitOfWorkPort};
use choreo_core::value_objects::{CeremonyId, CeremonyRevision};
use tokio::sync::RwLock;

#[tokio::test]
async fn the_in_memory_store_satisfies_the_transactional_contract() {
    let store = InMemoryCeremonyStore::new();

    let passed = CeremonyUnitOfWorkConformance::run(&store, &store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 6, "properties run: {passed:?}");
    assert!(passed.contains(&"a_stale_expectation_conflicts_and_changes_nothing"));
    assert!(passed.contains(&"concurrent_commits_admit_exactly_one_winner"));
}

#[tokio::test]
async fn the_suite_rejects_a_store_that_appends_before_it_checks() {
    let store = PiecewiseCeremonyStore::default();

    let failure = CeremonyUnitOfWorkConformance::run(&store, &store)
        .await
        .expect_err("a store that writes before checking must not pass");

    assert_eq!(
        failure.property(),
        "a_stale_expectation_conflicts_and_changes_nothing"
    );
}

/// A store that appends the journal first and checks the revision
/// afterwards.
///
/// Each part is written correctly; only their order is wrong. That is
/// what makes it the realistic failure — every individual operation
/// looks right in review, and the damage only appears when a commit is
/// rejected after half of it already landed.
#[derive(Debug, Default, Clone)]
struct PiecewiseCeremonyStore {
    inner: Arc<RwLock<BTreeMap<CeremonyId, Piecewise>>>,
}

#[derive(Debug, Default, Clone)]
struct Piecewise {
    revision: Option<CeremonyRevision>,
    instance: Option<CeremonyInstance>,
    journal: Vec<AuditRecord>,
}

#[async_trait]
impl CeremonyUnitOfWorkPort for PiecewiseCeremonyStore {
    async fn commit(&self, commit: CeremonyCommit) -> Result<CommitOutcome, DomainError> {
        let mut ceremonies = self.inner.write().await;
        let stored = ceremonies
            .entry(commit.instance().id().clone())
            .or_default();
        let (instance, expected, facts, _) = commit.into_parts();

        let mut sealed = Vec::new();
        for fact in facts {
            let record = match stored.journal.last() {
                Some(head) => AuditRecord::following(fact, head)?,
                None => AuditRecord::first(fact)?,
            };
            stored.journal.push(record.clone());
            sealed.push(record);
        }

        if !expected.matches(stored.revision) {
            return Ok(CommitOutcome::Conflict {
                expected,
                stored: stored.revision,
            });
        }

        let revision = expected.resulting_revision();
        stored.revision = Some(revision);
        stored.instance = Some(instance);
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
impl AuditJournalPort for PiecewiseCeremonyStore {
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

#[tokio::test]
async fn messages_are_enqueued_with_the_commit_and_never_without_it() {
    use choreo_core::entities::{AuditFact, CeremonyDefinition};
    use choreo_core::value_objects::{
        AuditActor, AuditActorKind, AuditEventType, CeremonyContext, CeremonyName, CeremonyState,
        CeremonyTransition, CeremonyVersion, EventId, ExpectedRevision, OutboxMessage,
        OutboxSubject, StateId, TransitionTrigger,
    };
    use serde_json::json;
    use time::OffsetDateTime;

    let store = InMemoryCeremonyStore::new();
    let ceremony = CeremonyId::new("outbox-ceremony").unwrap();
    let definition = CeremonyDefinition::new(
        CeremonyName::new("outbox_ceremony").unwrap(),
        CeremonyVersion::v1(),
        None,
        Vec::new(),
        Vec::new(),
        vec![
            CeremonyState::initial(StateId::new("OPEN").unwrap()),
            CeremonyState::terminal(StateId::new("DONE").unwrap()),
        ],
        vec![CeremonyTransition::new(
            StateId::new("OPEN").unwrap(),
            StateId::new("DONE").unwrap(),
            TransitionTrigger::new("finish").unwrap(),
            Vec::new(),
        )
        .unwrap()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    let build_commit = |expected: ExpectedRevision, event: &str| {
        CeremonyCommit::new(
            CeremonyInstance::start(
                ceremony.clone(),
                &definition,
                CeremonyContext::empty(),
                OffsetDateTime::UNIX_EPOCH,
            ),
            expected,
            [AuditFact {
                event_id: EventId::new(event).unwrap(),
                event_type: AuditEventType::CeremonyCompleted,
                ceremony_id: ceremony.clone(),
                definition_name: definition.name().clone(),
                definition_version: definition.version().clone(),
                occurred_at: OffsetDateTime::UNIX_EPOCH,
                actor: AuditActor::new("test", AuditActorKind::Engine, None).unwrap(),
                correlation_id: None,
                causation_id: None,
                trace: None,
            }],
            [OutboxMessage::new(
                EventId::new(event).unwrap(),
                OutboxSubject::new("choreo.ceremony.completed").unwrap(),
                json!({ "ceremony_id": ceremony.as_str() }),
                OffsetDateTime::UNIX_EPOCH,
            )
            .unwrap()],
        )
        .unwrap()
    };

    store
        .commit(build_commit(ExpectedRevision::New, "accepted"))
        .await
        .unwrap();
    assert_eq!(store.outbox(&ceremony).await.len(), 1);

    let outcome = store
        .commit(build_commit(ExpectedRevision::New, "rejected"))
        .await
        .unwrap();

    assert!(outcome.is_conflict());
    assert_eq!(
        store.outbox(&ceremony).await.len(),
        1,
        "a rejected commit enqueued its messages anyway"
    );
}
