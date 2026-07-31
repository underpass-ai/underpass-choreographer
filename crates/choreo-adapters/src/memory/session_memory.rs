//! An in-process memory backend.
//!
//! Real memory outlives the process that wrote it; this does not. It
//! exists so the engine has something to run against when no kernel is
//! configured, and so the conformance suite has a reference the rest
//! can be judged against.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    MemoryCapabilities, MemoryCapability, MemoryEntry, MemoryMoment, MemoryQuestion, MemoryScope,
};
use tokio::sync::RwLock;

#[derive(Debug, Default)]
struct Remembered {
    entries: Vec<MemoryEntry>,
    keys: BTreeSet<String>,
}

/// Memory that lives as long as the process does.
#[derive(Debug, Default)]
pub struct InProcessSessionMemory {
    scopes: RwLock<BTreeMap<String, Remembered>>,
}

impl InProcessSessionMemory {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything this backend does. Questions in words are not among
    /// them: answering one takes a reader that can weigh entries, and
    /// pretending otherwise by returning everything would be worse
    /// than saying no.
    #[must_use]
    fn declared() -> MemoryCapabilities {
        MemoryCapabilities::none()
            .with(MemoryCapability::Remembering)
            .with(MemoryCapability::Recalling)
            .with(MemoryCapability::TravellingInTime)
            .with(MemoryCapability::KeepingEvidence)
    }
}

#[async_trait]
impl MemoryWriterPort for InProcessSessionMemory {
    async fn remember(
        &self,
        scope: &MemoryScope,
        entries: Vec<MemoryEntry>,
        idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        if entries.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "memory.entries",
            });
        }
        let mut scopes = self.scopes.write().await;
        let remembered = scopes.entry(scope.as_str().to_owned()).or_default();
        if remembered.keys.contains(idempotency_key) {
            return Ok(MemoryWriteOutcome::AlreadyRemembered);
        }
        remembered.keys.insert(idempotency_key.to_owned());
        remembered.entries.extend(entries);
        Ok(MemoryWriteOutcome::Remembered)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        Self::declared()
    }
}

#[async_trait]
impl MemoryReaderPort for InProcessSessionMemory {
    async fn recall(&self, scope: &MemoryScope) -> Result<MemoryRecollection, DomainError> {
        let scopes = self.scopes.read().await;
        Ok(MemoryRecollection::Recalled(
            scopes
                .get(scope.as_str())
                .map(|remembered| remembered.entries.clone())
                .unwrap_or_default(),
        ))
    }

    async fn ask(
        &self,
        _scope: &MemoryScope,
        _question: &MemoryQuestion,
    ) -> Result<MemoryRecollection, DomainError> {
        Ok(MemoryRecollection::Unsupported)
    }

    async fn as_known_at(
        &self,
        scope: &MemoryScope,
        moment: MemoryMoment,
    ) -> Result<MemoryRecollection, DomainError> {
        let scopes = self.scopes.read().await;
        Ok(MemoryRecollection::Recalled(
            scopes
                .get(scope.as_str())
                .map(|remembered| {
                    remembered
                        .entries
                        .iter()
                        .filter(|entry| entry.provenance().observed_at() <= moment.instant())
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }

    fn capabilities(&self) -> MemoryCapabilities {
        Self::declared()
    }
}
