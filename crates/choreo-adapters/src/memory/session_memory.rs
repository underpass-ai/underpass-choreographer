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
    MemoryCapabilities, MemoryCapability, MemoryEntry, MemoryEntryId, MemoryMoment, MemoryQuestion,
    MemoryRelation, MemoryScope, MemoryWrite,
};
use tokio::sync::RwLock;

#[derive(Debug, Default)]
struct Remembered {
    entries: Vec<MemoryEntry>,
    relations: Vec<MemoryRelation>,
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
            .with(MemoryCapability::KeepingReasons)
            .with(MemoryCapability::FollowingReasons)
    }

    /// The shortest chain of reasons from `from` back to `to`.
    ///
    /// Breadth-first, so what comes back is the most direct
    /// explanation rather than the first one stumbled upon. Direction
    /// is followed as written: an edge says this came from that, and
    /// walking it backwards would turn a consequence into a cause.
    fn chain(
        relations: &[MemoryRelation],
        from: &MemoryEntryId,
        to: &MemoryEntryId,
    ) -> Vec<MemoryRelation> {
        let mut frontier = std::collections::VecDeque::from([from.clone()]);
        let mut arrived_by: BTreeMap<MemoryEntryId, MemoryRelation> = BTreeMap::new();
        let mut seen: BTreeSet<MemoryEntryId> = BTreeSet::from([from.clone()]);

        while let Some(here) = frontier.pop_front() {
            if &here == to {
                break;
            }
            for relation in relations.iter().filter(|relation| relation.from() == &here) {
                if seen.insert(relation.to().clone()) {
                    arrived_by.insert(relation.to().clone(), relation.clone());
                    frontier.push_back(relation.to().clone());
                }
            }
        }

        let mut chain = Vec::new();
        let mut here = to.clone();
        while let Some(relation) = arrived_by.get(&here) {
            chain.push(relation.clone());
            here = relation.from().clone();
        }
        chain.reverse();
        chain
    }

    /// A reason whose ends are not both visible is not returned.
    ///
    /// An edge pointing at an entry the caller cannot see is worse than
    /// no edge: it says an explanation exists and gives no way to reach
    /// it. This is what reading memory as of a moment needs, where the
    /// far end may not have been written yet.
    fn between(relations: &[MemoryRelation], visible: &[MemoryEntry]) -> Vec<MemoryRelation> {
        let ids: BTreeSet<&MemoryEntryId> = visible.iter().map(MemoryEntry::id).collect();
        relations
            .iter()
            .filter(|relation| ids.contains(relation.from()) && ids.contains(relation.to()))
            .cloned()
            .collect()
    }
}

#[async_trait]
impl MemoryWriterPort for InProcessSessionMemory {
    async fn remember(
        &self,
        scope: &MemoryScope,
        write: MemoryWrite,
        idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        let mut scopes = self.scopes.write().await;
        let remembered = scopes.entry(scope.as_str().to_owned()).or_default();
        if remembered.keys.contains(idempotency_key) {
            return Ok(MemoryWriteOutcome::AlreadyRemembered);
        }
        remembered.keys.insert(idempotency_key.to_owned());
        let (entries, relations) = write.into_parts();
        remembered.entries.extend(entries);
        remembered.relations.extend(relations);
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
        let Some(remembered) = scopes.get(scope.as_str()) else {
            return Ok(MemoryRecollection::nothing());
        };
        Ok(MemoryRecollection::Recalled {
            entries: remembered.entries.clone(),
            relations: remembered.relations.clone(),
        })
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
        let Some(remembered) = scopes.get(scope.as_str()) else {
            return Ok(MemoryRecollection::nothing());
        };
        let entries: Vec<MemoryEntry> = remembered
            .entries
            .iter()
            .filter(|entry| entry.provenance().observed_at() <= moment.instant())
            .cloned()
            .collect();
        let relations = Self::between(&remembered.relations, &entries);
        Ok(MemoryRecollection::Recalled { entries, relations })
    }

    async fn follow(
        &self,
        scope: &MemoryScope,
        from: &MemoryEntryId,
        to: &MemoryEntryId,
    ) -> Result<MemoryRecollection, DomainError> {
        let scopes = self.scopes.read().await;
        let Some(remembered) = scopes.get(scope.as_str()) else {
            return Ok(MemoryRecollection::nothing());
        };
        Ok(MemoryRecollection::Recalled {
            entries: Vec::new(),
            relations: Self::chain(&remembered.relations, from, to),
        })
    }

    fn capabilities(&self) -> MemoryCapabilities {
        Self::declared()
    }
}
