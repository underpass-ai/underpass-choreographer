use async_trait::async_trait;

use crate::error::DomainError;
use crate::value_objects::{
    MemoryCapabilities, MemoryEntry, MemoryEntryId, MemoryMoment, MemoryQuestion, MemoryRelation,
    MemoryScope,
};

/// What memory gave back.
///
/// `Unsupported` is a first-class answer rather than an error because
/// a backend that cannot travel in time is not misbehaving — it is a
/// smaller backend, and a caller told so plainly can offer the person
/// something else instead of showing them a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryRecollection {
    /// What was remembered, and what connects it.
    ///
    /// Both, always. Handing back entries alone would answer "what
    /// happened here" and silently refuse "how did this come about",
    /// and a caller has no way to tell a memory with no reasons from a
    /// reader that dropped them.
    Recalled {
        entries: Vec<MemoryEntry>,
        relations: Vec<MemoryRelation>,
    },
    /// The backend does not do this, and said so in its capabilities.
    Unsupported,
}

impl MemoryRecollection {
    /// Entries with nothing connecting them yet.
    #[must_use]
    pub fn of(entries: Vec<MemoryEntry>) -> Self {
        Self::Recalled {
            entries,
            relations: Vec::new(),
        }
    }

    /// Nothing is remembered here.
    #[must_use]
    pub fn nothing() -> Self {
        Self::of(Vec::new())
    }

    #[must_use]
    pub fn entries(&self) -> &[MemoryEntry] {
        match self {
            Self::Recalled { entries, .. } => entries,
            Self::Unsupported => &[],
        }
    }

    /// The reasons between what came back — the part that can be
    /// followed rather than only read.
    #[must_use]
    pub fn relations(&self) -> &[MemoryRelation] {
        match self {
            Self::Recalled { relations, .. } => relations,
            Self::Unsupported => &[],
        }
    }

    #[must_use]
    pub const fn is_supported(&self) -> bool {
        matches!(self, Self::Recalled { .. })
    }
}

/// Reading what earlier sessions learned, and why.
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

    /// The chain of reasons leading from `from` back to `to`.
    ///
    /// The question the whole contract exists to answer, and the only
    /// one whose failure means the memory has stopped being worth
    /// keeping: everything else can be reconstructed by reading, and
    /// this cannot.
    ///
    /// It answers with the reasons and not with the prose — the edges
    /// on the path, in the order they connect. What each end says is
    /// what `recall` is for, and a backend that padded the chain with
    /// text would make two contracts out of one.
    ///
    /// An empty chain is a real answer: the two are not connected by
    /// anything anyone wrote down.
    async fn follow(
        &self,
        scope: &MemoryScope,
        from: &MemoryEntryId,
        to: &MemoryEntryId,
    ) -> Result<MemoryRecollection, DomainError>;

    fn capabilities(&self) -> MemoryCapabilities;
}
