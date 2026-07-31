//! The memory ports' contract, run against every adapter that claims
//! to implement it — and against one that deliberately does not.
//!
//! A suite nobody has watched fail proves nothing. The counterexamples
//! below are the reason to believe the passes.

use async_trait::async_trait;
use choreo_adapters::memory::{ForgetfulMemory, InProcessSessionMemory};
use choreo_core::conformance::MemoryConformance;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    MemoryCapabilities, MemoryCapability, MemoryEntry, MemoryMoment, MemoryQuestion, MemoryScope,
};

#[tokio::test]
async fn in_process_memory_satisfies_the_contract() {
    let memory = InProcessSessionMemory::new();

    let passed = MemoryConformance::run(&memory, &memory)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 9, "{passed:?}");
}

/// A backend that keeps nothing is not a broken backend. It declares
/// nothing and refuses everything consistently, and the suite must let
/// it through — otherwise "no memory configured" would have no honest
/// implementation.
#[tokio::test]
async fn a_backend_that_declares_nothing_still_satisfies_the_contract() {
    let memory = ForgetfulMemory::new();

    let passed = MemoryConformance::run(&memory, &memory)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 9, "{passed:?}");
}

/// The failure mode the suite exists for: a backend that claims to
/// remember, answers `Remembered`, and quietly keeps nothing. Every
/// read then looks exactly like an empty scope.
#[derive(Debug, Default)]
struct MemoryThatForgetsQuietly;

#[async_trait]
impl MemoryWriterPort for MemoryThatForgetsQuietly {
    async fn remember(
        &self,
        _scope: &MemoryScope,
        entries: Vec<MemoryEntry>,
        _idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        if entries.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "memory.entries",
            });
        }
        Ok(MemoryWriteOutcome::Remembered)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryCapabilities::none()
            .with(MemoryCapability::Remembering)
            .with(MemoryCapability::Recalling)
    }
}

#[async_trait]
impl MemoryReaderPort for MemoryThatForgetsQuietly {
    async fn recall(&self, _scope: &MemoryScope) -> Result<MemoryRecollection, DomainError> {
        Ok(MemoryRecollection::Recalled(Vec::new()))
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
            .with(MemoryCapability::Remembering)
            .with(MemoryCapability::Recalling)
    }
}

#[tokio::test]
async fn the_suite_catches_a_backend_that_forgets_quietly() {
    let failure = MemoryConformance::run(&MemoryThatForgetsQuietly, &MemoryThatForgetsQuietly)
        .await
        .expect_err("a backend that keeps nothing must not pass");

    assert_eq!(failure.property(), "what_is_remembered_can_be_recalled");
    assert!(
        failure.detail().contains("came back as nothing"),
        "{failure}"
    );
}

/// The second failure mode: a backend that keeps everything but
/// answers `Remembered` to a retry, so a caller cannot tell a retry
/// that worked from a write it never sent — and the memory doubles.
#[derive(Debug, Default)]
struct MemoryThatDoublesOnRetry {
    inner: InProcessSessionMemory,
}

#[async_trait]
impl MemoryWriterPort for MemoryThatDoublesOnRetry {
    async fn remember(
        &self,
        scope: &MemoryScope,
        entries: Vec<MemoryEntry>,
        _idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        // A fresh key every time: the write is never recognised as one
        // already made.
        self.inner
            .remember(scope, entries, &uuid::Uuid::new_v4().to_string())
            .await
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryWriterPort::capabilities(&self.inner)
    }
}

#[async_trait]
impl MemoryReaderPort for MemoryThatDoublesOnRetry {
    async fn recall(&self, scope: &MemoryScope) -> Result<MemoryRecollection, DomainError> {
        self.inner.recall(scope).await
    }

    async fn ask(
        &self,
        scope: &MemoryScope,
        question: &MemoryQuestion,
    ) -> Result<MemoryRecollection, DomainError> {
        self.inner.ask(scope, question).await
    }

    async fn as_known_at(
        &self,
        scope: &MemoryScope,
        moment: MemoryMoment,
    ) -> Result<MemoryRecollection, DomainError> {
        self.inner.as_known_at(scope, moment).await
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryReaderPort::capabilities(&self.inner)
    }
}

#[tokio::test]
async fn the_suite_catches_a_backend_that_doubles_a_retried_write() {
    let memory = MemoryThatDoublesOnRetry::default();

    let failure = MemoryConformance::run(&memory, &memory)
        .await
        .expect_err("a backend that doubles a retry must not pass");

    assert_eq!(failure.property(), "the_same_write_twice_is_one_memory");
}

/// The third: a backend that says it can read memory as of a moment
/// and then hands back what was learned afterwards. This is the one
/// that would quietly ruin a judgement about a past decision.
#[derive(Debug, Default)]
struct MemoryThatCannotKeepTime {
    inner: InProcessSessionMemory,
}

#[async_trait]
impl MemoryWriterPort for MemoryThatCannotKeepTime {
    async fn remember(
        &self,
        scope: &MemoryScope,
        entries: Vec<MemoryEntry>,
        idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        self.inner.remember(scope, entries, idempotency_key).await
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryWriterPort::capabilities(&self.inner)
    }
}

#[async_trait]
impl MemoryReaderPort for MemoryThatCannotKeepTime {
    async fn recall(&self, scope: &MemoryScope) -> Result<MemoryRecollection, DomainError> {
        self.inner.recall(scope).await
    }

    async fn ask(
        &self,
        scope: &MemoryScope,
        question: &MemoryQuestion,
    ) -> Result<MemoryRecollection, DomainError> {
        self.inner.ask(scope, question).await
    }

    async fn as_known_at(
        &self,
        scope: &MemoryScope,
        _moment: MemoryMoment,
    ) -> Result<MemoryRecollection, DomainError> {
        // Ignores the moment entirely.
        self.inner.recall(scope).await
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryReaderPort::capabilities(&self.inner)
    }
}

#[tokio::test]
async fn the_suite_catches_a_backend_that_ignores_the_moment_asked_for() {
    let memory = MemoryThatCannotKeepTime::default();

    let failure = MemoryConformance::run(&memory, &memory)
        .await
        .expect_err("a backend that ignores the moment must not pass");

    assert_eq!(failure.property(), "time_travel_is_honoured_or_declined");
    assert!(failure.detail().contains("learned after it"), "{failure}");
}
