use async_trait::async_trait;

use crate::error::DomainError;
use crate::value_objects::{MemoryCapabilities, MemoryEntry, MemoryScope};

/// What became of a write.
///
/// Retrying is a normal thing for a caller to do — a network gave up,
/// a process restarted — and the second attempt must not double the
/// memory. Saying which of the two happened, rather than answering
/// "fine" both times, is what lets a caller tell a retry that worked
/// from a write it never sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryWriteOutcome {
    /// The entries are now in memory, put there by this call.
    Remembered,
    /// This exact write had already been made. Nothing changed.
    AlreadyRemembered,
    /// The backend does not keep memory, and said so in its
    /// capabilities. Not an error: a session with nowhere to record
    /// what it decided still runs, it just forgets.
    NotRemembered,
}

/// Writing what a working session decided into memory that outlives it.
///
/// The engine already keeps an audit journal, and this is not that. A
/// journal proves what happened in one session; memory is what a later
/// session can navigate. One is evidence, the other is experience.
#[async_trait]
pub trait MemoryWriterPort: Send + Sync {
    /// Record entries about `scope`.
    ///
    /// `idempotency_key` names the write, not the moment: the same key
    /// twice is the same write twice, whatever the clock says.
    async fn remember(
        &self,
        scope: &MemoryScope,
        entries: Vec<MemoryEntry>,
        idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError>;

    /// What this backend can do. A caller may ask before it acts, and
    /// the conformance suite checks the answer against behaviour: a
    /// backend that claims to remember and then does not is worse than
    /// one that claims nothing.
    fn capabilities(&self) -> MemoryCapabilities;
}
