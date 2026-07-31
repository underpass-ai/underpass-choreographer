use async_trait::async_trait;

use crate::error::DomainError;
use crate::value_objects::{
    MemoryCapabilities, MemoryEntry, MemoryMoment, MemoryQuestion, MemoryScope,
};

/// What memory gave back.
///
/// `Unsupported` is a first-class answer rather than an error because
/// a backend that cannot travel in time is not misbehaving — it is a
/// smaller backend, and a caller told so plainly can offer the person
/// something else instead of showing them a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryRecollection {
    Recalled(Vec<MemoryEntry>),
    /// The backend does not do this, and said so in its capabilities.
    Unsupported,
}

impl MemoryRecollection {
    #[must_use]
    pub fn entries(&self) -> &[MemoryEntry] {
        match self {
            Self::Recalled(entries) => entries,
            Self::Unsupported => &[],
        }
    }

    #[must_use]
    pub const fn is_supported(&self) -> bool {
        matches!(self, Self::Recalled(_))
    }
}

/// Reading what earlier sessions learned.
///
/// Three ways of asking, because three different questions get asked:
/// what is known about this at all, what does memory say about one
/// thing in particular, and what was known at a moment. The third is
/// not the first two filtered by date — it excludes what was learned
/// later about earlier events, which is the whole point of asking it.
#[async_trait]
pub trait MemoryReaderPort: Send + Sync {
    /// Everything memory holds about `scope`.
    async fn recall(&self, scope: &MemoryScope) -> Result<MemoryRecollection, DomainError>;

    /// What memory says in answer to a question put in words.
    async fn ask(
        &self,
        scope: &MemoryScope,
        question: &MemoryQuestion,
    ) -> Result<MemoryRecollection, DomainError>;

    /// What was known about `scope` at `moment`.
    async fn as_known_at(
        &self,
        scope: &MemoryScope,
        moment: MemoryMoment,
    ) -> Result<MemoryRecollection, DomainError>;

    fn capabilities(&self) -> MemoryCapabilities;
}
