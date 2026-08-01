//! [`SessionJournal`] — storing what a session did together with the
//! record of its having done it.
//!
//! State and audit are two claims about the same moment. Saved apart,
//! a process can die between them and leave a journal that disagrees
//! with what is stored — or worse, state with no journal at all, which
//! looks exactly like a session that was never audited. They go
//! through one call so they land together or not at all.
//!
//! # Read the revision before the session
//!
//! This is the whole safety of the thing and it is easy to get
//! backwards.
//!
//! Reading the session first and its revision second lets a write land
//! in between: the session in hand is then stale while the revision is
//! fresh, they agree at commit time, and the commit **overwrites
//! somebody's work in silence**.
//!
//! Reading the revision first inverts the failure. A write landing in
//! between leaves a fresh session paired with a stale expectation, the
//! commit is refused, and the caller reloads. The worst case becomes a
//! conflict that costs a retry, which is the correct shape for a race
//! to fail in.

use std::sync::Arc;

use choreo_core::entities::{AuditFact, CeremonyCommit, CeremonyInstance, CommitOutcome};
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyInstanceRepositoryPort, CeremonyUnitOfWorkPort};
use choreo_core::value_objects::{CeremonyId, ExpectedRevision};

/// A session as it was read, and what the store believed at the time.
///
/// The two travel together because using one without the other is the
/// mistake this type exists to prevent.
#[derive(Debug)]
pub struct LoadedSession {
    pub instance: CeremonyInstance,
    pub expected: ExpectedRevision,
}

/// Reads a session with its revision, and stores it with its facts.
pub struct SessionJournal {
    unit_of_work: Arc<dyn CeremonyUnitOfWorkPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
}

impl std::fmt::Debug for SessionJournal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SessionJournal").finish()
    }
}

impl SessionJournal {
    #[must_use]
    pub fn new(
        unit_of_work: Arc<dyn CeremonyUnitOfWorkPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    ) -> Self {
        Self {
            unit_of_work,
            instances,
        }
    }

    /// A session and the revision to commit it against.
    ///
    /// The revision is read first, deliberately. See this module's
    /// note: the other order turns a race into a silent overwrite,
    /// and this one turns it into a conflict.
    pub async fn load(&self, ceremony_id: &CeremonyId) -> Result<LoadedSession, DomainError> {
        let stored = self.unit_of_work.revision(ceremony_id).await?;
        let instance = self.instances.get(ceremony_id).await?;
        Ok(LoadedSession {
            instance,
            expected: stored.map_or(ExpectedRevision::New, ExpectedRevision::Exactly),
        })
    }

    /// Store the session and the facts it produced, all of it or none.
    ///
    /// A conflict is returned as one rather than as an invariant
    /// violation: the caller lost a race and should read again, which
    /// is a different instruction from "this can never work".
    pub async fn commit(
        &self,
        session: LoadedSession,
        facts: Vec<AuditFact>,
    ) -> Result<CeremonyInstance, DomainError> {
        let LoadedSession { instance, expected } = session;
        let commit = CeremonyCommit::new(instance.clone(), expected, facts, Vec::new())?;

        match self.unit_of_work.commit(commit).await? {
            CommitOutcome::Committed { .. } => Ok(instance),
            CommitOutcome::Conflict { .. } => Err(DomainError::Conflict {
                what: "ceremony_instance",
            }),
        }
    }

    /// Store a session that did not exist until now, with the facts
    /// that opening it produced.
    ///
    /// Committed against `New`, which is what makes opening atomic.
    /// Asking whether the session exists and then storing it leaves a
    /// gap: two starts of the same id both find nothing, both store,
    /// and the second silently replaces the first — no error raised,
    /// and the context the first caller opened with simply gone.
    ///
    /// The race surfaces as `AlreadyExists` rather than `Conflict`
    /// because that is what actually happened, and it is the answer a
    /// caller starting a session already knows how to handle.
    pub async fn open(
        &self,
        instance: CeremonyInstance,
        facts: Vec<AuditFact>,
    ) -> Result<CeremonyInstance, DomainError> {
        let commit =
            CeremonyCommit::new(instance.clone(), ExpectedRevision::New, facts, Vec::new())?;

        match self.unit_of_work.commit(commit).await? {
            CommitOutcome::Committed { .. } => Ok(instance),
            CommitOutcome::Conflict { .. } => Err(DomainError::AlreadyExists {
                what: "ceremony_instance",
            }),
        }
    }
}
