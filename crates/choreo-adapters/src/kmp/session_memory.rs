//! A working session's memory, kept by a memory kernel.
//!
//! # What is measured, and why it is the reasons
//!
//! Every call records how many **reasons** it carried, not only how
//! many entries. Entries alone always look healthy — a session that
//! writes ten observations and connects none of them writes ten
//! entries, same as a session that explains itself. The count of edges
//! is the number that falls when memory stops being worth keeping, and
//! it falls silently.
//!
//! Following is the one read the kernel itself measures the quality
//! of, so it is deliberately a real call to the kernel rather than a
//! walk over what a previous read returned. A memory nobody ever
//! traces is a memory nobody is measuring, on either side.

use std::collections::BTreeSet;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    MemoryCapabilities, MemoryCapability, MemoryEntry, MemoryEntryId, MemoryMoment, MemoryQuestion,
    MemoryRelation, MemoryScope, MemoryWrite,
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
            .with(MemoryCapability::KeepingReasons)
            .with(MemoryCapability::FollowingReasons)
    }

    /// Everything the kernel holds about `scope` as of `moment`.
    ///
    /// The moment is applied again here, on what came back. Paging
    /// continues from a reference rather than a time, and a filter
    /// that only the first page enforced would let a later page
    /// deliver what was learned after the moment asked about — the
    /// one thing reading memory as of a moment must never do.
    #[tracing::instrument(
        name = "kmp_read",
        skip_all,
        fields(
            scope = %scope,
            entries = tracing::field::Empty,
            reasons = tracing::field::Empty,
        )
    )]
    async fn read_as_of(
        &self,
        scope: &MemoryScope,
        moment: MemoryMoment,
    ) -> Result<(Vec<MemoryEntry>, Vec<MemoryRelation>), DomainError> {
        let mut collected = Vec::new();
        let mut reasons: Vec<MemoryRelation> = Vec::new();
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
                    return Ok((collected, reasons));
                }
                KernelAnswer::Refused(words) => return Err(refused(&words, "kernel_goto")),
            };

            let page = mapping::read_page(scope, &document);
            unreadable += page.unreadable;
            for reason in page.relations {
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
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
        // A reason whose ends are not both here would say an
        // explanation exists and give no way to reach it, which is
        // worse than saying nothing.
        let visible: BTreeSet<&MemoryEntryId> = collected.iter().map(MemoryEntry::id).collect();
        let reasons = reasons
            .iter()
            .filter(|reason| visible.contains(reason.from()) && visible.contains(reason.to()))
            .cloned()
            .collect::<Vec<_>>();
        // The pair that says whether this memory is worth having: a
        // read that is all nodes and no edges can be read and not
        // followed, and nothing else in the answer shows it.
        let span = tracing::Span::current();
        span.record("entries", collected.len());
        span.record("reasons", reasons.len());
        Ok((collected, reasons))
    }
}

#[async_trait]
impl<T: KernelTransport> MemoryWriterPort for KernelSessionMemory<T> {
    #[tracing::instrument(
        name = "kmp_remember",
        skip_all,
        fields(
            scope = %scope,
            entries = write.entries().len(),
            reasons = write.relations().len(),
            outcome = tracing::field::Empty,
        )
    )]
    async fn remember(
        &self,
        scope: &MemoryScope,
        write: MemoryWrite,
        idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        if idempotency_key.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory.idempotency_key",
            });
        }

        let arguments = mapping::ingest_arguments(scope, &write, idempotency_key)?;
        let answer = self
            .kernel
            .call("kernel_ingest", arguments)
            .await
            .map_err(|error| unreachable_kernel(&error, "kernel_ingest"))?;

        let outcome = match answer {
            KernelAnswer::Returned(_) => MemoryWriteOutcome::Remembered,
            KernelAnswer::Refused(words) if mapping::means_already_remembered(&words) => {
                MemoryWriteOutcome::AlreadyRemembered
            }
            KernelAnswer::Refused(words) => return Err(refused(&words, "kernel_ingest")),
        };
        tracing::Span::current().record("outcome", tracing::field::debug(outcome));
        Ok(outcome)
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
            .map(|(entries, relations)| MemoryRecollection::Recalled { entries, relations })
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
            .map(|(entries, relations)| MemoryRecollection::Recalled { entries, relations })
    }

    /// Ask the kernel how one entry came from another.
    ///
    /// Its own path-finding rather than a walk over what `recall`
    /// returned, for two reasons: it reaches past whatever a single
    /// read happened to bring back, and it is one of the three calls
    /// the kernel measures the quality of. A memory nobody ever traces
    /// is a memory nobody is measuring.
    #[tracing::instrument(
        name = "kmp_follow",
        skip_all,
        fields(scope = %scope, from = %from, to = %to, steps = tracing::field::Empty)
    )]
    async fn follow(
        &self,
        scope: &MemoryScope,
        from: &MemoryEntryId,
        to: &MemoryEntryId,
    ) -> Result<MemoryRecollection, DomainError> {
        let answer = self
            .kernel
            .call("kernel_trace", mapping::trace_arguments(scope, from, to))
            .await
            .map_err(|error| unreachable_kernel(&error, "kernel_trace"))?;

        let document = match answer {
            KernelAnswer::Returned(document) => document,
            KernelAnswer::Refused(words) if mapping::means_nothing_is_written(&words) => {
                return Ok(MemoryRecollection::nothing());
            }
            KernelAnswer::Refused(words) => return Err(refused(&words, "kernel_trace")),
        };

        let chain = mapping::read_chain(scope, &document);
        // Zero steps between two entries that ought to be connected is
        // the shape of memory that has quietly stopped explaining
        // itself, so it is recorded rather than returned in silence.
        tracing::Span::current().record("steps", chain.len());
        Ok(MemoryRecollection::Recalled {
            entries: Vec::new(),
            relations: chain,
        })
    }

    fn capabilities(&self) -> MemoryCapabilities {
        Self::declared()
    }
}
