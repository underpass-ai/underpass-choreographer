//! Conformance suite for [`CeremonyUnitOfWorkPort`].
//!
//! Atomicity is a statement about the state *and* the journal at the
//! same time, so the suite takes both sides. An implementation that
//! only satisfies one of them is exactly what the unit of work exists
//! to prevent, and could not be caught by checking either alone.
//!
//! # What this suite cannot check
//!
//! **Crash atomicity.** These properties show that a rejected commit
//! leaves nothing behind when the process survives. Whether a commit
//! interrupted mid-write leaves nothing behind is a property of the
//! store, and the host proves it against its own.

use futures::future::join_all;
use time::OffsetDateTime;

use crate::entities::{
    AuditChain, AuditFact, CeremonyCommit, CeremonyDefinition, CeremonyInstance,
};
use crate::error::DomainError;
use crate::ports::{AuditJournalPort, CeremonyUnitOfWorkPort};
use crate::value_objects::{
    AuditActor, AuditActorKind, AuditEventType, CeremonyContext, CeremonyId, CeremonyName,
    CeremonyRevision, CeremonyState, CeremonyTransition, CeremonyVersion, EventId,
    ExpectedRevision, StateId, TransitionTrigger,
};

use super::ConformanceFailure;

const CONCURRENT_COMMITS: usize = 8;

/// Every property a [`CeremonyUnitOfWorkPort`] implementation must
/// satisfy, checked against the journal it shares.
#[derive(Debug)]
pub struct CeremonyUnitOfWorkConformance;

impl CeremonyUnitOfWorkConformance {
    /// Run the whole suite. Both ports must be backed by the same
    /// store, or atomicity cannot be observed.
    pub async fn run(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
        journal: &dyn AuditJournalPort,
    ) -> Result<Vec<&'static str>, ConformanceFailure> {
        let mut passed = Vec::new();
        Self::an_unstored_ceremony_has_no_revision(unit_of_work).await?;
        passed.push("an_unstored_ceremony_has_no_revision");
        Self::a_first_commit_reaches_the_initial_revision(unit_of_work).await?;
        passed.push("a_first_commit_reaches_the_initial_revision");
        Self::a_stale_expectation_conflicts_and_changes_nothing(unit_of_work, journal).await?;
        passed.push("a_stale_expectation_conflicts_and_changes_nothing");
        Self::successive_commits_advance_the_revision_by_one(unit_of_work).await?;
        passed.push("successive_commits_advance_the_revision_by_one");
        Self::facts_from_separate_commits_form_one_intact_chain(unit_of_work, journal).await?;
        passed.push("facts_from_separate_commits_form_one_intact_chain");
        Self::concurrent_commits_admit_exactly_one_winner(unit_of_work).await?;
        passed.push("concurrent_commits_admit_exactly_one_winner");
        Ok(passed)
    }

    async fn an_unstored_ceremony_has_no_revision(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "an_unstored_ceremony_has_no_revision";
        let ceremony = ceremony_id(PROPERTY, "unstored")?;

        if call(PROPERTY, unit_of_work.revision(&ceremony).await)?.is_some() {
            return Err(failure(
                PROPERTY,
                "a ceremony that was never committed reports a revision",
            ));
        }
        Ok(())
    }

    async fn a_first_commit_reaches_the_initial_revision(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_first_commit_reaches_the_initial_revision";
        let ceremony = ceremony_id(PROPERTY, "first")?;

        let outcome = call(
            PROPERTY,
            unit_of_work
                .commit(commit(PROPERTY, &ceremony, ExpectedRevision::New, 1)?)
                .await,
        )?;

        match outcome.committed_revision() {
            Some(revision) if revision == CeremonyRevision::INITIAL => {}
            Some(revision) => {
                return Err(failure(
                    PROPERTY,
                    format!("a first commit reached revision {}", revision.value()),
                ))
            }
            None => return Err(failure(PROPERTY, "a first commit conflicted")),
        }

        if outcome.records().len() != 1 {
            return Err(failure(
                PROPERTY,
                format!("expected 1 sealed record, got {}", outcome.records().len()),
            ));
        }
        Ok(())
    }

    /// The property the unit of work exists for: a rejected commit must
    /// not leave half of itself behind.
    async fn a_stale_expectation_conflicts_and_changes_nothing(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_stale_expectation_conflicts_and_changes_nothing";
        let ceremony = ceremony_id(PROPERTY, "stale")?;

        call(
            PROPERTY,
            unit_of_work
                .commit(commit(PROPERTY, &ceremony, ExpectedRevision::New, 1)?)
                .await,
        )?;
        let records_before = call(PROPERTY, journal.records(&ceremony).await)?.len();

        let outcome = call(
            PROPERTY,
            unit_of_work
                .commit(commit(PROPERTY, &ceremony, ExpectedRevision::New, 2)?)
                .await,
        )?;
        if !outcome.is_conflict() {
            return Err(failure(
                PROPERTY,
                "committing over an existing ceremony as new was accepted",
            ));
        }

        let revision = call(PROPERTY, unit_of_work.revision(&ceremony).await)?;
        if revision != Some(CeremonyRevision::INITIAL) {
            return Err(failure(
                PROPERTY,
                format!("a rejected commit moved the revision to {revision:?}"),
            ));
        }

        let records_after = call(PROPERTY, journal.records(&ceremony).await)?.len();
        if records_after != records_before {
            return Err(failure(
                PROPERTY,
                format!(
                    "a rejected commit still appended to the journal: {records_before} -> {records_after}"
                ),
            ));
        }
        Ok(())
    }

    async fn successive_commits_advance_the_revision_by_one(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "successive_commits_advance_the_revision_by_one";
        let ceremony = ceremony_id(PROPERTY, "advancing")?;

        let mut expected = ExpectedRevision::New;
        for ordinal in 1..=4_u64 {
            let outcome = call(
                PROPERTY,
                unit_of_work
                    .commit(commit(PROPERTY, &ceremony, expected, ordinal)?)
                    .await,
            )?;
            let revision = outcome.committed_revision().ok_or_else(|| {
                failure(
                    PROPERTY,
                    format!("commit {ordinal} conflicted unexpectedly"),
                )
            })?;
            if revision.value() != ordinal {
                return Err(failure(
                    PROPERTY,
                    format!("commit {ordinal} reached revision {}", revision.value()),
                ));
            }
            expected = ExpectedRevision::Exactly(revision);
        }
        Ok(())
    }

    async fn facts_from_separate_commits_form_one_intact_chain(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
        journal: &dyn AuditJournalPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "facts_from_separate_commits_form_one_intact_chain";
        let ceremony = ceremony_id(PROPERTY, "chaining")?;

        let mut expected = ExpectedRevision::New;
        for ordinal in 1..=3_u64 {
            let outcome = call(
                PROPERTY,
                unit_of_work
                    .commit(commit(PROPERTY, &ceremony, expected, ordinal)?)
                    .await,
            )?;
            expected = ExpectedRevision::Exactly(
                outcome
                    .committed_revision()
                    .ok_or_else(|| failure(PROPERTY, "a commit conflicted unexpectedly"))?,
            );
        }

        let records = call(PROPERTY, journal.records(&ceremony).await)?;
        if records.len() != 3 {
            return Err(failure(
                PROPERTY,
                format!(
                    "expected 3 records across 3 commits, found {}",
                    records.len()
                ),
            ));
        }
        let verdict = AuditChain::verify(&records);
        if !verdict.is_intact() {
            return Err(failure(
                PROPERTY,
                format!(
                    "records sealed across commits do not chain: {:?}",
                    verdict.defect()
                ),
            ));
        }
        Ok(())
    }

    async fn concurrent_commits_admit_exactly_one_winner(
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "concurrent_commits_admit_exactly_one_winner";
        let ceremony = ceremony_id(PROPERTY, "racing")?;

        let mut pending = Vec::with_capacity(CONCURRENT_COMMITS);
        for ordinal in 1..=CONCURRENT_COMMITS as u64 {
            pending.push(unit_of_work.commit(commit(
                PROPERTY,
                &ceremony,
                ExpectedRevision::New,
                ordinal,
            )?));
        }

        let mut committed = 0;
        for outcome in join_all(pending).await {
            if call(PROPERTY, outcome)?.committed_revision().is_some() {
                committed += 1;
            }
        }

        if committed != 1 {
            return Err(failure(
                PROPERTY,
                format!("{committed} concurrent commits were accepted, expected exactly 1"),
            ));
        }

        let revision = call(PROPERTY, unit_of_work.revision(&ceremony).await)?;
        if revision != Some(CeremonyRevision::INITIAL) {
            return Err(failure(
                PROPERTY,
                format!("the stored revision advanced past the single winner: {revision:?}"),
            ));
        }
        Ok(())
    }
}

fn call<T>(
    property: &'static str,
    outcome: Result<T, DomainError>,
) -> Result<T, ConformanceFailure> {
    outcome.map_err(|error| failure(property, format!("the adapter returned an error: {error}")))
}

fn failure(property: &'static str, detail: impl Into<String>) -> ConformanceFailure {
    ConformanceFailure::new(property, detail)
}

fn ceremony_id(property: &'static str, suffix: &str) -> Result<CeremonyId, ConformanceFailure> {
    CeremonyId::new(format!("conformance-{property}-{suffix}")).map_err(|error| {
        failure(
            property,
            format!("the suite built an invalid ceremony id: {error}"),
        )
    })
}

fn commit(
    property: &'static str,
    ceremony_id: &CeremonyId,
    expected: ExpectedRevision,
    ordinal: u64,
) -> Result<CeremonyCommit, ConformanceFailure> {
    let build = || -> Result<CeremonyCommit, DomainError> {
        let definition = definition()?;
        let instance = CeremonyInstance::start(
            ceremony_id.clone(),
            &definition,
            CeremonyContext::empty(),
            OffsetDateTime::UNIX_EPOCH,
        );
        let fact = AuditFact {
            event_id: EventId::new(format!("{property}-{ordinal}"))?,
            event_type: AuditEventType::StepCompleted,
            ceremony_id: ceremony_id.clone(),
            definition_name: definition.name().clone(),
            definition_version: definition.version().clone(),
            occurred_at: OffsetDateTime::UNIX_EPOCH,
            actor: AuditActor::new("conformance", AuditActorKind::Engine, None)?,
            correlation_id: None,
            causation_id: None,
            trace: None,
        };
        CeremonyCommit::new(instance, expected, [fact], [])
    };
    build().map_err(|error| {
        failure(
            property,
            format!("the suite built an invalid commit: {error}"),
        )
    })
}

fn definition() -> Result<CeremonyDefinition, DomainError> {
    CeremonyDefinition::new(
        CeremonyName::new("conformance_ceremony")?,
        CeremonyVersion::v1(),
        None,
        Vec::new(),
        Vec::new(),
        vec![
            CeremonyState::initial(StateId::new("OPEN")?),
            CeremonyState::terminal(StateId::new("DONE")?),
        ],
        vec![CeremonyTransition::new(
            StateId::new("OPEN")?,
            StateId::new("DONE")?,
            TransitionTrigger::new("finish")?,
            Vec::new(),
        )?],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}
