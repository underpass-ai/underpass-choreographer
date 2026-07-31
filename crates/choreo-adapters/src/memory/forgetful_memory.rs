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
    MemoryCapabilities, MemoryEntryId, MemoryMoment, MemoryQuestion, MemoryScope, MemoryWrite,
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
        _write: MemoryWrite,
        _idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
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

    async fn follow(
        &self,
        _scope: &MemoryScope,
        _from: &MemoryEntryId,
        _to: &MemoryEntryId,
    ) -> Result<MemoryRecollection, DomainError> {
        Ok(MemoryRecollection::Unsupported)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryCapabilities::none()
    }
}
