//! In-memory [`AuditJournalPort`] implementation.
//!
//! The reference implementation of the contract, and the one the
//! conformance suite is developed against. It is durable for exactly as
//! long as the process lives, which makes it right for tests and for
//! proving the port is implementable, and wrong for anything that has
//! to survive a restart.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::{AuditFact, AuditRecord};
use choreo_core::error::DomainError;
use choreo_core::ports::AuditJournalPort;
use choreo_core::value_objects::CeremonyId;
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct InMemoryAuditJournal {
    inner: Arc<RwLock<BTreeMap<CeremonyId, Vec<AuditRecord>>>>,
}

impl InMemoryAuditJournal {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn ceremonies(&self) -> usize {
        self.inner.read().await.len()
    }
}

#[async_trait]
impl AuditJournalPort for InMemoryAuditJournal {
    /// Reading the head, sealing and appending happen under one write
    /// lock. Releasing it between the read and the write would let two
    /// callers seal against the same predecessor and fork the chain.
    async fn append(&self, fact: AuditFact) -> Result<AuditRecord, DomainError> {
        let mut journals = self.inner.write().await;
        let journal = journals.entry(fact.ceremony_id.clone()).or_default();
        let record = match journal.last() {
            Some(head) => AuditRecord::following(fact, head)?,
            None => AuditRecord::first(fact)?,
        };
        journal.push(record.clone());
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
