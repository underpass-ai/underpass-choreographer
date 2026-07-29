//! The in-memory audit journal against the contract suite the engine
//! ships — and a deliberately broken adapter against the same suite, so
//! the suite is shown to detect what it claims to detect.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_adapters::memory::InMemoryAuditJournal;
use choreo_core::conformance::AuditJournalConformance;
use choreo_core::entities::{AuditFact, AuditRecord};
use choreo_core::error::DomainError;
use choreo_core::ports::AuditJournalPort;
use choreo_core::value_objects::CeremonyId;
use tokio::sync::RwLock;

#[tokio::test]
async fn the_in_memory_journal_satisfies_the_contract() {
    let journal = InMemoryAuditJournal::new();

    let passed = AuditJournalConformance::run(&journal)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 6, "properties run: {passed:?}");
    assert!(passed.contains(&"concurrent_appends_do_not_fork_the_chain"));
}

#[tokio::test]
async fn the_suite_rejects_a_journal_that_seals_outside_its_write_lock() {
    let journal = ForkableAuditJournal::default();

    let failure = AuditJournalConformance::run(&journal)
        .await
        .expect_err("a journal that forks its chain must not pass");

    assert_eq!(
        failure.property(),
        "concurrent_appends_do_not_fork_the_chain"
    );
}

/// An adapter that reads its head, yields, and only then writes.
///
/// This is the realistic mistake: the lock is taken twice instead of
/// held across the read and the write, so two callers seal against the
/// same predecessor. It exists here to keep the conformance suite
/// honest — a suite nothing fails proves nothing.
#[derive(Debug, Default, Clone)]
struct ForkableAuditJournal {
    inner: Arc<RwLock<BTreeMap<CeremonyId, Vec<AuditRecord>>>>,
}

#[async_trait]
impl AuditJournalPort for ForkableAuditJournal {
    async fn append(&self, fact: AuditFact) -> Result<AuditRecord, DomainError> {
        let ceremony_id = fact.ceremony_id.clone();
        let head = self
            .inner
            .read()
            .await
            .get(&ceremony_id)
            .and_then(|journal| journal.last())
            .cloned();

        tokio::task::yield_now().await;

        let record = match head {
            Some(head) => AuditRecord::following(fact, &head)?,
            None => AuditRecord::first(fact)?,
        };
        self.inner
            .write()
            .await
            .entry(ceremony_id)
            .or_default()
            .push(record.clone());
        Ok(record)
    }

    async fn head(&self, ceremony_id: &CeremonyId) -> Result<Option<AuditRecord>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(ceremony_id)
            .and_then(|journal| journal.last())
            .cloned())
    }

    async fn records(&self, ceremony_id: &CeremonyId) -> Result<Vec<AuditRecord>, DomainError> {
        Ok(self
            .inner
            .read()
            .await
            .get(ceremony_id)
            .cloned()
            .unwrap_or_default())
    }
}
