//! Conformance suite for [`AuditJournalPort`].
//!
//! The engine cedes durability to the host but not the contract. An
//! adapter that does not pass this suite does not implement the port,
//! whatever it stores. Without it, "the host provides persistence"
//! would mean nobody guarantees anything.
//!
//! # What this suite cannot check
//!
//! **Crash recovery.** By definition an in-memory adapter loses
//! everything on restart, and a suite that ran the same properties
//! against every implementation could never assert survival. Whether a
//! journal survives the process is the host's to prove, with its own
//! store and its own harness.
//!
//! **Parallel contention.** The concurrency property drives many
//! appends concurrently on the caller's runtime, which interleaves them
//! at every await point — where a read-then-write race in an async
//! adapter actually appears. Genuine multi-threaded contention against
//! a shared store is again the host's to exercise.

use std::fmt;

use futures::future::join_all;
use time::OffsetDateTime;

use crate::entities::{AuditChain, AuditFact, AuditRecord};
use crate::error::DomainError;
use crate::ports::AuditJournalPort;
use crate::value_objects::{
    AuditActor, AuditActorKind, AuditEventType, CeremonyId, CeremonyName, CeremonyVersion, EventId,
};

/// Number of appends the concurrency property drives at once.
const CONCURRENT_APPENDS: usize = 16;

/// A property the adapter under test failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceFailure {
    property: &'static str,
    detail: String,
}

impl ConformanceFailure {
    pub(super) fn new(property: &'static str, detail: impl Into<String>) -> Self {
        Self {
            property,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub fn property(&self) -> &'static str {
        self.property
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ConformanceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "audit journal conformance failed: {} — {}",
            self.property, self.detail
        )
    }
}

impl std::error::Error for ConformanceFailure {}

/// Every property an [`AuditJournalPort`] implementation must satisfy.
#[derive(Debug)]
pub struct AuditJournalConformance;

impl AuditJournalConformance {
    /// Run the whole suite, returning the properties that passed.
    ///
    /// The adapter must be empty. Each property uses its own ceremony
    /// identifiers, so a shared store is fine as long as nothing else
    /// writes to those ceremonies while the suite runs.
    pub async fn run(
        journal: &dyn AuditJournalPort,
    ) -> Result<Vec<&'static str>, ConformanceFailure> {
        let mut passed = Vec::new();
        Self::an_unwritten_journal_is_empty(journal).await?;
        passed.push("an_unwritten_journal_is_empty");
        Self::the_first_append_opens_the_chain(journal).await?;
        passed.push("the_first_append_opens_the_chain");
        Self::appends_chain_and_verify_intact(journal).await?;
        passed.push("appends_chain_and_verify_intact");
        Self::records_come_back_in_written_order(journal).await?;
        passed.push("records_come_back_in_written_order");
        Self::ceremonies_have_independent_journals(journal).await?;
        passed.push("ceremonies_have_independent_journals");
        Self::concurrent_appends_do_not_fork_the_chain(journal).await?;
        passed.push("concurrent_appends_do_not_fork_the_chain");
        Ok(passed)
    }

    async fn an_unwritten_journal_is_empty(
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "an_unwritten_journal_is_empty";
        let ceremony = ceremony_id(PROPERTY, "unwritten")?;

        let head = call(PROPERTY, journal.head(&ceremony).await)?;
        if head.is_some() {
            return Err(failure(
                PROPERTY,
                "a ceremony that was never written has a head",
            ));
        }

        let records = call(PROPERTY, journal.records(&ceremony).await)?;
        if !records.is_empty() {
            return Err(failure(
                PROPERTY,
                format!("expected no records, found {}", records.len()),
            ));
        }
        Ok(())
    }

    async fn the_first_append_opens_the_chain(
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "the_first_append_opens_the_chain";
        let ceremony = ceremony_id(PROPERTY, "opening")?;

        let record = call(
            PROPERTY,
            journal.append(fact(PROPERTY, &ceremony, 1)?).await,
        )?;

        if !record.sequence().is_first() {
            return Err(failure(
                PROPERTY,
                format!(
                    "the first record must sit at sequence 1, found {}",
                    record.sequence().value()
                ),
            ));
        }
        if record.previous_record_hash().is_some() {
            return Err(failure(
                PROPERTY,
                "the first record must not name a predecessor",
            ));
        }
        Ok(())
    }

    async fn appends_chain_and_verify_intact(
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "appends_chain_and_verify_intact";
        let ceremony = ceremony_id(PROPERTY, "chaining")?;

        let mut appended = Vec::new();
        for ordinal in 1..=5 {
            appended.push(call(
                PROPERTY,
                journal.append(fact(PROPERTY, &ceremony, ordinal)?).await,
            )?);
        }

        for pair in appended.windows(2) {
            if !pair[1].continues(&pair[0]) {
                return Err(failure(
                    PROPERTY,
                    format!(
                        "record {} does not continue record {}",
                        pair[1].sequence().value(),
                        pair[0].sequence().value()
                    ),
                ));
            }
        }

        let stored = call(PROPERTY, journal.records(&ceremony).await)?;
        verify_intact(PROPERTY, &stored)?;

        let head = call(PROPERTY, journal.head(&ceremony).await)?;
        match head {
            Some(head) if head.record_hash() == appended[appended.len() - 1].record_hash() => {
                Ok(())
            }
            Some(_) => Err(failure(PROPERTY, "head is not the last appended record")),
            None => Err(failure(PROPERTY, "a written journal reports no head")),
        }
    }

    async fn records_come_back_in_written_order(
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "records_come_back_in_written_order";
        let ceremony = ceremony_id(PROPERTY, "ordering")?;

        for ordinal in 1..=4 {
            call(
                PROPERTY,
                journal.append(fact(PROPERTY, &ceremony, ordinal)?).await,
            )?;
        }

        let stored = call(PROPERTY, journal.records(&ceremony).await)?;
        for (index, record) in stored.iter().enumerate() {
            let expected = index as u64 + 1;
            if record.sequence().value() != expected {
                return Err(failure(
                    PROPERTY,
                    format!(
                        "position {index} holds sequence {}, expected {expected}",
                        record.sequence().value()
                    ),
                ));
            }
        }
        Ok(())
    }

    async fn ceremonies_have_independent_journals(
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "ceremonies_have_independent_journals";
        let left = ceremony_id(PROPERTY, "left")?;
        let right = ceremony_id(PROPERTY, "right")?;

        call(PROPERTY, journal.append(fact(PROPERTY, &left, 1)?).await)?;
        call(PROPERTY, journal.append(fact(PROPERTY, &left, 2)?).await)?;
        let first_on_the_right = call(PROPERTY, journal.append(fact(PROPERTY, &right, 1)?).await)?;

        if !first_on_the_right.sequence().is_first() {
            return Err(failure(
                PROPERTY,
                "a second ceremony's journal must open at sequence 1",
            ));
        }
        if first_on_the_right.previous_record_hash().is_some() {
            return Err(failure(
                PROPERTY,
                "a second ceremony's first record must not link into another journal",
            ));
        }

        let stored = call(PROPERTY, journal.records(&left).await)?;
        if stored.len() != 2 {
            return Err(failure(
                PROPERTY,
                format!("expected 2 records, found {}", stored.len()),
            ));
        }
        if stored.iter().any(|record| record.ceremony_id() != &left) {
            return Err(failure(
                PROPERTY,
                "a journal returned another ceremony's records",
            ));
        }
        Ok(())
    }

    /// The property that separates an atomic adapter from one that
    /// reads the head, awaits, and then writes.
    async fn concurrent_appends_do_not_fork_the_chain(
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "concurrent_appends_do_not_fork_the_chain";
        let ceremony = ceremony_id(PROPERTY, "concurrent")?;

        let mut pending = Vec::with_capacity(CONCURRENT_APPENDS);
        for ordinal in 1..=CONCURRENT_APPENDS {
            pending.push(journal.append(fact(PROPERTY, &ceremony, ordinal as u64)?));
        }
        for outcome in join_all(pending).await {
            call(PROPERTY, outcome)?;
        }

        let stored = call(PROPERTY, journal.records(&ceremony).await)?;
        if stored.len() != CONCURRENT_APPENDS {
            return Err(failure(
                PROPERTY,
                format!(
                    "expected {CONCURRENT_APPENDS} records, found {} — appends were lost or duplicated",
                    stored.len()
                ),
            ));
        }
        verify_intact(PROPERTY, &stored)
    }
}

fn verify_intact(
    property: &'static str,
    records: &[AuditRecord],
) -> Result<(), ConformanceFailure> {
    let verdict = AuditChain::verify(records);
    if verdict.is_intact() {
        return Ok(());
    }
    Err(failure(
        property,
        format!("the stored chain does not verify: {:?}", verdict.defect()),
    ))
}

fn call<T>(
    property: &'static str,
    outcome: Result<T, DomainError>,
) -> Result<T, ConformanceFailure> {
    outcome.map_err(|error| failure(property, format!("the adapter returned an error: {error}")))
}

fn failure(property: &'static str, detail: impl Into<String>) -> ConformanceFailure {
    ConformanceFailure {
        property,
        detail: detail.into(),
    }
}

fn ceremony_id(property: &'static str, suffix: &str) -> Result<CeremonyId, ConformanceFailure> {
    CeremonyId::new(format!("conformance-{property}-{suffix}")).map_err(|error| {
        failure(
            property,
            format!("the suite built an invalid ceremony id: {error}"),
        )
    })
}

fn fact(
    property: &'static str,
    ceremony_id: &CeremonyId,
    ordinal: u64,
) -> Result<AuditFact, ConformanceFailure> {
    let build = || -> Result<AuditFact, DomainError> {
        Ok(AuditFact {
            event_id: EventId::new(format!("{property}-{ordinal}"))?,
            event_type: AuditEventType::StepCompleted,
            ceremony_id: ceremony_id.clone(),
            definition_name: CeremonyName::new("conformance_ceremony")?,
            definition_version: CeremonyVersion::v1(),
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            actor: AuditActor::new("conformance", AuditActorKind::Engine, None)?,
            correlation_id: None,
            causation_id: None,
            trace: None,
        })
    };
    build().map_err(|error| {
        failure(
            property,
            format!("the suite built an invalid fact: {error}"),
        )
    })
}
