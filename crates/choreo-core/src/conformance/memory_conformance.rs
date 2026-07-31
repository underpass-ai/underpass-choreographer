//! Conformance suite for [`MemoryWriterPort`] and [`MemoryReaderPort`].
//!
//! Memory is the one port where an adapter can look like it works and
//! not: a backend that silently drops writes answers every read with
//! nothing, and nothing is exactly what an empty scope looks like. The
//! properties here are written to tell those two apart.
//!
//! The suite is capability-driven on purpose. A backend that declares
//! nothing is legitimate — it is the honest shape of "no memory
//! configured" — so what is checked is not that a backend does
//! everything, but that **it does what it says it does**. Claiming a
//! capability and not having it is the failure; having less than
//! everything is not.
//!
//! # What this suite cannot check
//!
//! **Whether the memory is any good.** That entries are stored and
//! come back says nothing about whether a later session can navigate
//! them. Quality is the kernel's to measure, and it does.
//!
//! **Survival.** An in-process backend loses everything on restart, so
//! no property runnable against every implementation could assert it.

use std::fmt;

use time::OffsetDateTime;

use crate::error::DomainError;
use crate::ports::{MemoryReaderPort, MemoryWriteOutcome, MemoryWriterPort};
use crate::value_objects::{
    Attributes, CeremonyId, MemoryEntry, MemoryEntryKind, MemoryEvidence, MemoryMoment,
    MemoryProvenance, MemoryQuestion, MemoryScope,
};

/// A property the adapter under test failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryConformanceFailure {
    property: &'static str,
    detail: String,
}

impl MemoryConformanceFailure {
    fn new(property: &'static str, detail: impl Into<String>) -> Self {
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

impl fmt::Display for MemoryConformanceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "memory conformance failed: {} — {}",
            self.property, self.detail
        )
    }
}

impl std::error::Error for MemoryConformanceFailure {}

type Checked = Result<(), MemoryConformanceFailure>;

fn scope(name: &str) -> MemoryScope {
    MemoryScope::new(format!("ceremony:conformance-{name}")).expect("scope should be valid")
}

fn entry(summary: &str, kind: MemoryEntryKind, observed_at: OffsetDateTime) -> MemoryEntry {
    MemoryEntry::new(
        kind,
        summary,
        None,
        MemoryProvenance::new(
            CeremonyId::new("conformance").expect("ceremony id should be valid"),
            None,
            observed_at,
        ),
        Attributes::empty(),
    )
    .expect("entry should be valid")
}

fn moment(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(seconds)
}

/// Every property a memory adapter must satisfy.
#[derive(Debug)]
pub struct MemoryConformance;

impl MemoryConformance {
    /// Run the whole suite, returning the properties that passed.
    ///
    /// The adapter must be empty. Each property uses its own scope, so
    /// a shared backend is fine as long as nothing else writes to
    /// those scopes while the suite runs.
    pub async fn run(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Result<Vec<&'static str>, MemoryConformanceFailure> {
        let mut passed = Vec::new();

        Self::capabilities_are_stable(writer, reader)?;
        passed.push("capabilities_are_stable");

        Self::an_unwritten_scope_recalls_nothing(reader).await?;
        passed.push("an_unwritten_scope_recalls_nothing");

        Self::what_is_remembered_can_be_recalled(writer, reader).await?;
        passed.push("what_is_remembered_can_be_recalled");

        Self::scopes_do_not_bleed_into_each_other(writer, reader).await?;
        passed.push("scopes_do_not_bleed_into_each_other");

        Self::the_same_write_twice_is_one_memory(writer, reader).await?;
        passed.push("the_same_write_twice_is_one_memory");

        Self::an_empty_write_is_refused(writer).await?;
        passed.push("an_empty_write_is_refused");

        Self::evidence_survives_the_round_trip(writer, reader).await?;
        passed.push("evidence_survives_the_round_trip");

        Self::questions_are_answered_or_declined(writer, reader).await?;
        passed.push("questions_are_answered_or_declined");

        Self::time_travel_is_honoured_or_declined(writer, reader).await?;
        passed.push("time_travel_is_honoured_or_declined");

        Ok(passed)
    }

    /// Capabilities are read more than once, and a backend whose
    /// answer wanders cannot be relied on for any of the checks below.
    fn capabilities_are_stable(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "capabilities_are_stable";
        if writer.capabilities() != writer.capabilities() {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                "the writer answered its own capabilities differently twice",
            ));
        }
        if reader.capabilities() != reader.capabilities() {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                "the reader answered its own capabilities differently twice",
            ));
        }
        Ok(())
    }

    async fn an_unwritten_scope_recalls_nothing(reader: &dyn MemoryReaderPort) -> Checked {
        const PROPERTY: &str = "an_unwritten_scope_recalls_nothing";
        let recollection = reader
            .recall(&scope("unwritten"))
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        if !reader.capabilities().recalls() {
            return expect_unsupported(PROPERTY, &recollection, "recall");
        }
        if recollection.entries().is_empty() {
            Ok(())
        } else {
            Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!(
                    "a scope nobody wrote to came back with {} entries",
                    recollection.entries().len()
                ),
            ))
        }
    }

    /// The property that tells a working backend from one that drops
    /// writes: both answer an empty scope with nothing, and only one
    /// answers a written scope with something.
    async fn what_is_remembered_can_be_recalled(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "what_is_remembered_can_be_recalled";
        let scope = scope("round-trip");
        let outcome = writer
            .remember(
                &scope,
                vec![entry(
                    "the rollback was rehearsed in March",
                    MemoryEntryKind::Observation,
                    moment(10),
                )],
                "conformance:round-trip",
            )
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        if !writer.capabilities().remembers() {
            return if outcome == MemoryWriteOutcome::NotRemembered {
                Ok(())
            } else {
                Err(MemoryConformanceFailure::new(
                    PROPERTY,
                    format!("a backend that does not remember answered {outcome:?}"),
                ))
            };
        }
        if outcome != MemoryWriteOutcome::Remembered {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!("a first write answered {outcome:?}"),
            ));
        }
        if !reader.capabilities().recalls() {
            return Ok(());
        }

        let recalled = reader
            .recall(&scope)
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        match recalled.entries() {
            [only] if only.summary() == "the rollback was rehearsed in March" => Ok(()),
            [] => Err(MemoryConformanceFailure::new(
                PROPERTY,
                "what was written came back as nothing — a dropped write and an empty scope must not look the same",
            )),
            others => Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!("expected exactly what was written, got {} entries", others.len()),
            )),
        }
    }

    async fn scopes_do_not_bleed_into_each_other(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "scopes_do_not_bleed_into_each_other";
        if !writer.capabilities().remembers() || !reader.capabilities().recalls() {
            return Ok(());
        }
        let mine = scope("mine");
        let yours = scope("yours");
        writer
            .remember(
                &mine,
                vec![entry("mine", MemoryEntryKind::Decision, moment(1))],
                "conformance:mine",
            )
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        writer
            .remember(
                &yours,
                vec![entry("yours", MemoryEntryKind::Decision, moment(2))],
                "conformance:yours",
            )
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        let recalled = reader
            .recall(&mine)
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        if recalled.entries().iter().any(|e| e.summary() == "yours") {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                "one scope's memory surfaced in another's",
            ));
        }
        Ok(())
    }

    /// A retry must not double the memory, and must say which of the
    /// two happened. Answering "remembered" to both makes a caller
    /// unable to tell a successful retry from a write it never sent.
    async fn the_same_write_twice_is_one_memory(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "the_same_write_twice_is_one_memory";
        if !writer.capabilities().remembers() {
            return Ok(());
        }
        let scope = scope("retried");
        let entries = || vec![entry("decided once", MemoryEntryKind::Decision, moment(5))];

        let first = writer
            .remember(&scope, entries(), "conformance:retried")
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        let second = writer
            .remember(&scope, entries(), "conformance:retried")
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        if first != MemoryWriteOutcome::Remembered {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!("a first write answered {first:?}"),
            ));
        }
        if second != MemoryWriteOutcome::AlreadyRemembered {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!("the same write repeated answered {second:?}, not AlreadyRemembered"),
            ));
        }
        if !reader.capabilities().recalls() {
            return Ok(());
        }
        let recalled = reader
            .recall(&scope)
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        if recalled.entries().len() == 1 {
            Ok(())
        } else {
            Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!(
                    "one write made twice left {} entries",
                    recalled.entries().len()
                ),
            ))
        }
    }

    /// Writing nothing is a call that would change nothing, and a
    /// backend that answers "remembered" to it is lying quietly.
    async fn an_empty_write_is_refused(writer: &dyn MemoryWriterPort) -> Checked {
        const PROPERTY: &str = "an_empty_write_is_refused";
        match writer
            .remember(&scope("empty"), Vec::new(), "conformance:empty")
            .await
        {
            Err(DomainError::EmptyCollection { .. }) => Ok(()),
            Err(other) => Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!("an empty write failed with {other}, not EmptyCollection"),
            )),
            Ok(outcome) => Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!("an empty write was accepted as {outcome:?}"),
            )),
        }
    }

    async fn evidence_survives_the_round_trip(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "evidence_survives_the_round_trip";
        if !writer.capabilities().keeps_evidence() {
            return Ok(());
        }
        let scope = scope("evidence");
        let evidenced = entry(
            "the queue was empty at 03:20",
            MemoryEntryKind::Observation,
            moment(20),
        )
        .with_evidence(vec![MemoryEvidence::new(
            "dead-letter count",
            Some("dead-letter-queue".to_owned()),
            Attributes::empty(),
        )
        .expect("evidence should be valid")]);

        writer
            .remember(&scope, vec![evidenced], "conformance:evidence")
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        if !reader.capabilities().recalls() {
            return Ok(());
        }
        let recalled = reader
            .recall(&scope)
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        match recalled.entries().first() {
            Some(entry) if entry.evidence().len() == 1 => Ok(()),
            Some(entry) => Err(MemoryConformanceFailure::new(
                PROPERTY,
                format!(
                    "a backend that keeps evidence returned an entry with {} of it",
                    entry.evidence().len()
                ),
            )),
            None => Err(MemoryConformanceFailure::new(
                PROPERTY,
                "the evidenced entry did not come back at all",
            )),
        }
    }

    async fn questions_are_answered_or_declined(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "questions_are_answered_or_declined";
        let scope = scope("asked");
        if writer.capabilities().remembers() {
            writer
                .remember(
                    &scope,
                    vec![entry(
                        "we restarted the ingester",
                        MemoryEntryKind::Outcome,
                        moment(30),
                    )],
                    "conformance:asked",
                )
                .await
                .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        }

        let question =
            MemoryQuestion::new("did we restart the ingester?").expect("question should be valid");
        let answer = reader
            .ask(&scope, &question)
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        if reader.capabilities().answers_questions() {
            Ok(())
        } else {
            expect_unsupported(PROPERTY, &answer, "ask")
        }
    }

    async fn time_travel_is_honoured_or_declined(
        writer: &dyn MemoryWriterPort,
        reader: &dyn MemoryReaderPort,
    ) -> Checked {
        const PROPERTY: &str = "time_travel_is_honoured_or_declined";
        let scope = scope("as-known-at");
        if writer.capabilities().remembers() {
            writer
                .remember(
                    &scope,
                    vec![
                        entry("known early", MemoryEntryKind::Observation, moment(100)),
                        entry("known later", MemoryEntryKind::Observation, moment(900)),
                    ],
                    "conformance:as-known-at",
                )
                .await
                .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;
        }

        let recalled = reader
            .as_known_at(&scope, MemoryMoment::at(moment(500)))
            .await
            .map_err(|error| MemoryConformanceFailure::new(PROPERTY, error.to_string()))?;

        if !reader.capabilities().travels_in_time() {
            return expect_unsupported(PROPERTY, &recalled, "as_known_at");
        }
        if !writer.capabilities().remembers() {
            return Ok(());
        }
        if recalled
            .entries()
            .iter()
            .any(|e| e.summary() == "known later")
        {
            return Err(MemoryConformanceFailure::new(
                PROPERTY,
                "reading memory as of a moment returned something learned after it",
            ));
        }
        if recalled
            .entries()
            .iter()
            .any(|e| e.summary() == "known early")
        {
            Ok(())
        } else {
            Err(MemoryConformanceFailure::new(
                PROPERTY,
                "reading memory as of a moment lost what was already known then",
            ))
        }
    }
}

fn expect_unsupported(
    property: &'static str,
    recollection: &crate::ports::MemoryRecollection,
    operation: &str,
) -> Checked {
    if recollection.is_supported() {
        Err(MemoryConformanceFailure::new(
            property,
            format!("`{operation}` answered as supported by a backend that does not declare it"),
        ))
    } else {
        Ok(())
    }
}
