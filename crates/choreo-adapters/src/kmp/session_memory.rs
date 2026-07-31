//! A working session's memory, kept by a memory kernel.

use std::collections::BTreeSet;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    MemoryCapabilities, MemoryCapability, MemoryEntry, MemoryMoment, MemoryQuestion, MemoryScope,
};

use super::error::{refused, unreachable_kernel};
use super::mapping;
use super::transport::{KernelAnswer, KernelTransport};

/// How many pages of one temporal read to walk before giving up.
///
/// A guard against a kernel that keeps promising more, not a limit on
/// how much a session may remember: at the page size in use it allows
/// far more memory than a working session produces. Reaching it is
/// logged, because a bound that quietly truncates reads exactly like
/// a session that never said much.
const MAX_PAGES: usize = 100;

/// Memory kept by a kernel outside this process.
#[derive(Debug)]
pub struct KernelSessionMemory<T> {
    kernel: T,
}

impl<T: KernelTransport> KernelSessionMemory<T> {
    #[must_use]
    pub const fn new(kernel: T) -> Self {
        Self { kernel }
    }

    /// What this backend does, and what it does not.
    ///
    /// Questions in words are left out although the kernel answers
    /// them well. What it gives back is an answer with its proof, and
    /// this port hands back entries; returning the entries a proof
    /// happens to cite would quietly redefine what asking means for
    /// every other backend. Declining is reversible, and redefining a
    /// contract from inside one adapter is not.
    #[must_use]
    fn declared() -> MemoryCapabilities {
        MemoryCapabilities::none()
            .with(MemoryCapability::Remembering)
            .with(MemoryCapability::Recalling)
            .with(MemoryCapability::TravellingInTime)
            .with(MemoryCapability::KeepingEvidence)
    }

    /// Everything the kernel holds about `scope` as of `moment`.
    ///
    /// The moment is applied again here, on what came back. Paging
    /// continues from a reference rather than a time, and a filter
    /// that only the first page enforced would let a later page
    /// deliver what was learned after the moment asked about — the
    /// one thing reading memory as of a moment must never do.
    async fn read_as_of(
        &self,
        scope: &MemoryScope,
        moment: MemoryMoment,
    ) -> Result<Vec<MemoryEntry>, DomainError> {
        let mut collected = Vec::new();
        let mut seen = BTreeSet::new();
        let mut unreadable = 0;
        let mut cursor: Option<String> = None;

        for page_number in 0..MAX_PAGES {
            let arguments = mapping::goto_arguments(scope, moment, cursor.as_deref())?;
            let answer = self
                .kernel
                .call("kernel_goto", arguments)
                .await
                .map_err(|error| unreachable_kernel(&error, "kernel_goto"))?;

            let document = match answer {
                KernelAnswer::Returned(document) => document,
                KernelAnswer::Refused(words) if mapping::means_nothing_is_written(&words) => {
                    return Ok(collected);
                }
                KernelAnswer::Refused(words) => return Err(refused(&words, "kernel_goto")),
            };

            let page = mapping::read_page(scope, &document);
            unreadable += page.unreadable;
            for (reference, entry) in page.entries {
                if entry.provenance().observed_at() > moment.instant() {
                    continue;
                }
                if seen.insert(reference) {
                    collected.push(entry);
                }
            }

            match page.next_cursor {
                Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
                _ => break,
            }
            if page_number + 1 == MAX_PAGES {
                tracing::warn!(
                    scope = scope.as_str(),
                    pages = MAX_PAGES,
                    "stopped reading memory before the kernel ran out of it"
                );
            }
        }

        if unreadable > 0 {
            tracing::warn!(
                scope = scope.as_str(),
                unreadable,
                "memory this engine cannot represent was left out of a recollection"
            );
        }
        Ok(collected)
    }
}

#[async_trait]
impl<T: KernelTransport> MemoryWriterPort for KernelSessionMemory<T> {
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
        if idempotency_key.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory.idempotency_key",
            });
        }

        let arguments = mapping::ingest_arguments(scope, &entries, idempotency_key)?;
        let answer = self
            .kernel
            .call("kernel_ingest", arguments)
            .await
            .map_err(|error| unreachable_kernel(&error, "kernel_ingest"))?;

        match answer {
            KernelAnswer::Returned(_) => Ok(MemoryWriteOutcome::Remembered),
            KernelAnswer::Refused(words) if mapping::means_already_remembered(&words) => {
                Ok(MemoryWriteOutcome::AlreadyRemembered)
            }
            KernelAnswer::Refused(words) => Err(refused(&words, "kernel_ingest")),
        }
    }

    fn capabilities(&self) -> MemoryCapabilities {
        Self::declared()
    }
}

#[async_trait]
impl<T: KernelTransport> MemoryReaderPort for KernelSessionMemory<T> {
    async fn recall(&self, scope: &MemoryScope) -> Result<MemoryRecollection, DomainError> {
        self.read_as_of(scope, mapping::end_of_time())
            .await
            .map(MemoryRecollection::Recalled)
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
        self.read_as_of(scope, moment)
            .await
            .map(MemoryRecollection::Recalled)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        Self::declared()
    }
}
