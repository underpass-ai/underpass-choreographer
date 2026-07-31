//! A memory backend that keeps nothing, and says so.
//!
//! The honest shape of "no kernel configured". A session with nowhere
//! to record what it decided still runs; it just forgets, and every
//! caller can tell that from the capabilities before it asks.

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    MemoryCapabilities, MemoryEntry, MemoryMoment, MemoryQuestion, MemoryScope,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ForgetfulMemory;

impl ForgetfulMemory {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl MemoryWriterPort for ForgetfulMemory {
    async fn remember(
        &self,
        _scope: &MemoryScope,
        entries: Vec<MemoryEntry>,
        _idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        // Refused before it is dropped: a caller who sent nothing is
        // told so whether or not anyone was going to keep it.
        if entries.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "memory.entries",
            });
        }
        Ok(MemoryWriteOutcome::NotRemembered)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryCapabilities::none()
    }
}

#[async_trait]
impl MemoryReaderPort for ForgetfulMemory {
    async fn recall(&self, _scope: &MemoryScope) -> Result<MemoryRecollection, DomainError> {
        Ok(MemoryRecollection::Unsupported)
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
        _scope: &MemoryScope,
        _moment: MemoryMoment,
    ) -> Result<MemoryRecollection, DomainError> {
        Ok(MemoryRecollection::Unsupported)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryCapabilities::none()
    }
}
