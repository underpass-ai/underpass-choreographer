//! The embedded redb store against every contract the engine defines.
//!
//! The point of building the suites before the store: this file asserts
//! almost nothing of its own. What makes the store correct is that it
//! satisfies the same properties as the reference implementation, and
//! that is checked rather than argued.

#![cfg(feature = "redb")]

use choreo_adapters::redb::RedbCeremonyStore;
use choreo_core::conformance::{
    AuditJournalConformance, CeremonyDefinitionPublicationConformance,
    CeremonySessionStoreConformance, CeremonyUnitOfWorkConformance, OutboxConformance,
};
use tempfile::TempDir;

/// Each suite gets its own database: several of them require a store
/// that nothing else has written to.
fn store() -> (TempDir, RedbCeremonyStore) {
    let directory = TempDir::new().expect("a temporary directory");
    let store =
        RedbCeremonyStore::open(directory.path().join("ceremonies.redb")).expect("the store opens");
    (directory, store)
}

#[tokio::test]
async fn redb_satisfies_the_audit_journal_contract() {
    let (_directory, store) = store();

    let passed = AuditJournalConformance::run(&store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 6, "properties run: {passed:?}");
}

#[tokio::test]
async fn redb_satisfies_the_transactional_contract() {
    let (_directory, store) = store();

    let passed = CeremonyUnitOfWorkConformance::run(&store, &store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 6, "properties run: {passed:?}");
}

#[tokio::test]
async fn redb_satisfies_the_outbox_contract() {
    let (_directory, store) = store();

    let passed = OutboxConformance::run(&store, &store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 7, "properties run: {passed:?}");
}

#[tokio::test]
async fn redb_satisfies_the_publication_contract() {
    let (_directory, store) = store();

    let passed = CeremonyDefinitionPublicationConformance::run(&store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 5, "properties run: {passed:?}");
}

/// What no in-memory adapter can be asked: does anything survive the
/// store being closed and opened again?
#[tokio::test]
async fn a_reopened_store_still_holds_its_journal_and_verifies() {
    use choreo_core::entities::AuditChain;
    use choreo_core::ports::{AuditJournalPort, CeremonyUnitOfWorkPort};
    use choreo_core::value_objects::{CeremonyId, CeremonyRevision, ExpectedRevision};

    let directory = TempDir::new().expect("a temporary directory");
    let path = directory.path().join("ceremonies.redb");
    let ceremony = CeremonyId::new("survives-restart").unwrap();

    {
        let store = RedbCeremonyStore::open(&path).expect("the store opens");
        let mut expected = ExpectedRevision::New;
        for ordinal in 1..=3_u64 {
            let outcome = store
                .commit(support::commit(&ceremony, expected, ordinal))
                .await
                .unwrap();
            expected = ExpectedRevision::Exactly(outcome.committed_revision().unwrap());
        }
    }

    let reopened = RedbCeremonyStore::open(&path).expect("the store reopens");

    assert_eq!(
        reopened.revision(&ceremony).await.unwrap(),
        Some(CeremonyRevision::new(3).unwrap()),
        "the revision did not survive reopening"
    );
    let records = reopened.records(&ceremony).await.unwrap();
    assert_eq!(records.len(), 3);
    assert!(
        AuditChain::verify(&records).is_intact(),
        "the chain did not survive reopening"
    );
}

mod support {
    use choreo_core::entities::{AuditFact, CeremonyCommit, CeremonyDefinition, CeremonyInstance};
    use choreo_core::value_objects::{
        AuditActor, AuditActorKind, AuditEventType, CeremonyContext, CeremonyId, CeremonyName,
        CeremonyState, CeremonyTransition, CeremonyVersion, EventId, ExpectedRevision, StateId,
        TransitionTrigger,
    };
    use time::OffsetDateTime;

    pub fn instance(ceremony_id: &CeremonyId) -> CeremonyInstance {
        CeremonyInstance::start(
            ceremony_id.clone(),
            &definition(),
            CeremonyContext::empty(),
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    pub fn commit(
        ceremony_id: &CeremonyId,
        expected: ExpectedRevision,
        ordinal: u64,
    ) -> CeremonyCommit {
        let definition = definition();
        let instance = CeremonyInstance::start(
            ceremony_id.clone(),
            &definition,
            CeremonyContext::empty(),
            OffsetDateTime::UNIX_EPOCH,
        );
        let fact = AuditFact {
            event_id: EventId::new(format!("restart-{ordinal}")).unwrap(),
            event_type: AuditEventType::StepCompleted,
            ceremony_id: ceremony_id.clone(),
            definition_name: definition.name().clone(),
            definition_version: definition.version().clone(),
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            actor: AuditActor::new("test", AuditActorKind::Engine, None).unwrap(),
            correlation_id: None,
            causation_id: None,
            trace: None,
        };
        CeremonyCommit::new(instance, expected, [fact], []).unwrap()
    }

    fn definition() -> CeremonyDefinition {
        CeremonyDefinition::new(
            CeremonyName::new("restart_ceremony").unwrap(),
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
        .unwrap()
    }
}

#[tokio::test]
async fn instances_survive_reopening_the_store() {
    use choreo_core::ports::CeremonyInstanceRepositoryPort;
    use choreo_core::value_objects::CeremonyId;

    let directory = TempDir::new().expect("a temporary directory");
    let path = directory.path().join("ceremonies.redb");
    let ceremony = CeremonyId::new("survives-as-instance").unwrap();

    {
        let store = RedbCeremonyStore::open(&path).expect("the store opens");
        store
            .save(&support::instance(&ceremony))
            .await
            .expect("the instance is stored");
    }

    let reopened = RedbCeremonyStore::open(&path).expect("the store reopens");

    assert!(reopened.exists(&ceremony).await.unwrap());
    assert_eq!(reopened.get(&ceremony).await.unwrap().id(), &ceremony);
    assert_eq!(reopened.list().await.unwrap().len(), 1);
}

/// The repository port carries no expected revision — it has nowhere to
/// put one. Advancing the revision on every save is what stops that
/// weaker path from quietly defeating the transactional one: a commit
/// still holding the revision it read now conflicts, as it should.
#[tokio::test]
async fn saving_outside_a_unit_of_work_makes_a_stale_commit_conflict() {
    use choreo_core::ports::{CeremonyInstanceRepositoryPort, CeremonyUnitOfWorkPort};
    use choreo_core::value_objects::{CeremonyId, ExpectedRevision};

    let (_directory, store) = store();
    let ceremony = CeremonyId::new("racing-paths").unwrap();

    let committed = store
        .commit(support::commit(&ceremony, ExpectedRevision::New, 1))
        .await
        .unwrap();
    let observed = committed.committed_revision().unwrap();

    // Someone writes through the repository while the caller above
    // still believes it holds the current revision.
    store.save(&support::instance(&ceremony)).await.unwrap();

    let outcome = store
        .commit(support::commit(
            &ceremony,
            ExpectedRevision::Exactly(observed),
            2,
        ))
        .await
        .unwrap();

    assert!(
        outcome.is_conflict(),
        "a commit against a revision that was overwritten was accepted"
    );
}

#[tokio::test]
async fn it_serves_both_session_ports_over_one_storage() {
    let (_directory, store) = store();

    let passed = CeremonySessionStoreConformance::run(&store, &store)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 3, "properties run: {passed:?}");
}
