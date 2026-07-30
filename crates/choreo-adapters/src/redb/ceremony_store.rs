//! [`RedbCeremonyStore`] — the embedded durable store.
//!
//! Ceremony state, the audit journal and the outbox live in three
//! tables of one redb database, so a commit that touches all three is
//! one write transaction. Collaborating stores with a transaction each
//! would satisfy every property except the one that matters.
//!
//! redb is synchronous. Every operation runs on the blocking pool
//! rather than inline: a store call that blocks the async executor is
//! invisible until a host is under load, and then it is very visible.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use choreo_core::entities::{
    AuditFact, AuditRecord, CeremonyCommit, CeremonyInstance, CommitOutcome, PublicationOutcome,
    PublishedCeremonyDefinition,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    AuditJournalPort, CeremonyDefinitionPublicationPort, CeremonyUnitOfWorkPort, OutboxPort,
};
use choreo_core::value_objects::{
    CeremonyDefinitionDigest, CeremonyId, CeremonyName, CeremonyRevision, CeremonyVersion,
    ClaimedOutboxMessage, DurationMs, EventId, OutboxAttempt, OutboxMessage,
    OutboxQuarantineReason,
};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::error::{encoding_failure, join_failure, store_failure};
use super::keys::{ceremony_of, published, scope_range, scoped};

const CEREMONIES: TableDefinition<&str, &[u8]> = TableDefinition::new("ceremony_instances");
const JOURNAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("audit_journal");
const OUTBOX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("outbox");
const PUBLICATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("published_definitions");

/// A ceremony's stored state and the revision that guards it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCeremony {
    revision: CeremonyRevision,
    instance: CeremonyInstance,
}

/// A committed message and everything the store knows about getting it
/// out.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredOutboxMessage {
    message: OutboxMessage,
    attempt: OutboxAttempt,
    #[serde(default, with = "time::serde::rfc3339::option")]
    claimed_until: Option<OffsetDateTime>,
    delivered: bool,
    quarantine: Option<OutboxQuarantineReason>,
}

impl StoredOutboxMessage {
    fn enqueued(message: OutboxMessage) -> Self {
        Self {
            message,
            attempt: OutboxAttempt::NONE,
            claimed_until: None,
            delivered: false,
            quarantine: None,
        }
    }

    fn is_claimable(&self, now: OffsetDateTime) -> bool {
        !self.delivered
            && self.quarantine.is_none()
            && self.claimed_until.is_none_or(|until| until <= now)
    }
}

#[derive(Debug, Clone)]
pub struct RedbCeremonyStore {
    database: Arc<Database>,
}

impl RedbCeremonyStore {
    /// Open, creating the database and its tables when absent.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let database =
            Database::create(path).map_err(|error| store_failure(error, "open database"))?;
        let write = database
            .begin_write()
            .map_err(|error| store_failure(error, "open tables"))?;
        {
            write
                .open_table(CEREMONIES)
                .map_err(|error| store_failure(error, "open ceremonies table"))?;
            write
                .open_table(JOURNAL)
                .map_err(|error| store_failure(error, "open journal table"))?;
            write
                .open_table(OUTBOX)
                .map_err(|error| store_failure(error, "open outbox table"))?;
            write
                .open_table(PUBLICATIONS)
                .map_err(|error| store_failure(error, "open publications table"))?;
        }
        write
            .commit()
            .map_err(|error| store_failure(error, "create tables"))?;
        Ok(Self {
            database: Arc::new(database),
        })
    }

    async fn blocking<T, F>(&self, op: &'static str, work: F) -> Result<T, DomainError>
    where
        T: Send + 'static,
        F: FnOnce(&Database) -> Result<T, DomainError> + Send + 'static,
    {
        let database = Arc::clone(&self.database);
        tokio::task::spawn_blocking(move || work(&database))
            .await
            .map_err(|error| join_failure(&error, op))?
    }
}

fn encode<T: Serialize>(value: &T, op: &'static str) -> Result<Vec<u8>, DomainError> {
    serde_json::to_vec(value).map_err(|error| encoding_failure(&error, op))
}

fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8], op: &'static str) -> Result<T, DomainError> {
    serde_json::from_slice(bytes).map_err(|error| encoding_failure(&error, op))
}

fn journal_of(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    ceremony_id: &CeremonyId,
) -> Result<Vec<AuditRecord>, DomainError> {
    let (start, end) = scope_range(ceremony_id);
    let mut records = Vec::new();
    for entry in table
        .range(start.as_slice()..=end.as_slice())
        .map_err(|error| store_failure(error, "scan journal"))?
    {
        let (_, value) = entry.map_err(|error| store_failure(error, "read journal entry"))?;
        records.push(decode(value.value(), "decode audit record")?);
    }
    Ok(records)
}

#[async_trait]
impl CeremonyUnitOfWorkPort for RedbCeremonyStore {
    /// State, journal and outbox are written in one write transaction:
    /// redb commits all three tables together or none of them.
    async fn commit(&self, commit: CeremonyCommit) -> Result<CommitOutcome, DomainError> {
        self.blocking("commit", move |database| {
            let ceremony_id = commit.instance().id().clone();
            let (instance, expected, facts, messages) = commit.into_parts();

            let write = database
                .begin_write()
                .map_err(|error| store_failure(error, "begin commit"))?;
            let outcome = {
                let mut ceremonies = write
                    .open_table(CEREMONIES)
                    .map_err(|error| store_failure(error, "open ceremonies"))?;
                let stored: Option<StoredCeremony> = ceremonies
                    .get(ceremony_id.as_str())
                    .map_err(|error| store_failure(error, "read ceremony"))?
                    .map(|value| decode(value.value(), "decode ceremony"))
                    .transpose()?;
                let stored_revision = stored.map(|stored| stored.revision);

                if expected.matches(stored_revision) {
                    let mut journal = write
                        .open_table(JOURNAL)
                        .map_err(|error| store_failure(error, "open journal"))?;
                    let mut head = journal_of(&journal, &ceremony_id)?.pop();
                    let mut sealed = Vec::with_capacity(facts.len());
                    for fact in facts {
                        let record = match &head {
                            Some(previous) => AuditRecord::following(fact, previous)?,
                            None => AuditRecord::first(fact)?,
                        };
                        journal
                            .insert(
                                scoped(&ceremony_id, record.sequence().value()).as_slice(),
                                encode(&record, "encode audit record")?.as_slice(),
                            )
                            .map_err(|error| store_failure(error, "append journal"))?;
                        head = Some(record.clone());
                        sealed.push(record);
                    }

                    let mut outbox = write
                        .open_table(OUTBOX)
                        .map_err(|error| store_failure(error, "open outbox"))?;
                    let (start, end) = scope_range(&ceremony_id);
                    let enqueued = outbox
                        .range(start.as_slice()..=end.as_slice())
                        .map_err(|error| store_failure(error, "scan outbox"))?
                        .count() as u64;
                    for (offset, message) in messages.into_iter().enumerate() {
                        outbox
                            .insert(
                                scoped(&ceremony_id, enqueued + offset as u64).as_slice(),
                                encode(
                                    &StoredOutboxMessage::enqueued(message),
                                    "encode outbox message",
                                )?
                                .as_slice(),
                            )
                            .map_err(|error| store_failure(error, "enqueue message"))?;
                    }

                    let revision = expected.resulting_revision();
                    ceremonies
                        .insert(
                            ceremony_id.as_str(),
                            encode(
                                &StoredCeremony {
                                    revision,
                                    instance: instance.clone(),
                                },
                                "encode ceremony",
                            )?
                            .as_slice(),
                        )
                        .map_err(|error| store_failure(error, "store ceremony"))?;

                    CommitOutcome::Committed {
                        revision,
                        records: sealed,
                    }
                } else {
                    // Dropping the transaction without committing is
                    // what makes a rejected commit leave nothing behind.
                    CommitOutcome::Conflict {
                        expected,
                        stored: stored_revision,
                    }
                }
            };

            if outcome.is_conflict() {
                return Ok(outcome);
            }
            write
                .commit()
                .map_err(|error| store_failure(error, "commit"))?;
            Ok(outcome)
        })
        .await
    }

    async fn revision(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Result<Option<CeremonyRevision>, DomainError> {
        let ceremony_id = ceremony_id.clone();
        self.blocking("revision", move |database| {
            let read = database
                .begin_read()
                .map_err(|error| store_failure(error, "begin read"))?;
            let ceremonies = read
                .open_table(CEREMONIES)
                .map_err(|error| store_failure(error, "open ceremonies"))?;
            let stored: Option<StoredCeremony> = ceremonies
                .get(ceremony_id.as_str())
                .map_err(|error| store_failure(error, "read ceremony"))?
                .map(|value| decode(value.value(), "decode ceremony"))
                .transpose()?;
            Ok(stored.map(|stored| stored.revision))
        })
        .await
    }
}

#[async_trait]
impl AuditJournalPort for RedbCeremonyStore {
    async fn append(&self, fact: AuditFact) -> Result<AuditRecord, DomainError> {
        self.blocking("append", move |database| {
            let ceremony_id = fact.ceremony_id.clone();
            let write = database
                .begin_write()
                .map_err(|error| store_failure(error, "begin append"))?;
            let record = {
                let mut journal = write
                    .open_table(JOURNAL)
                    .map_err(|error| store_failure(error, "open journal"))?;
                let head = journal_of(&journal, &ceremony_id)?.pop();
                let record = match head {
                    Some(previous) => AuditRecord::following(fact, &previous)?,
                    None => AuditRecord::first(fact)?,
                };
                journal
                    .insert(
                        scoped(&ceremony_id, record.sequence().value()).as_slice(),
                        encode(&record, "encode audit record")?.as_slice(),
                    )
                    .map_err(|error| store_failure(error, "append journal"))?;
                record
            };
            write
                .commit()
                .map_err(|error| store_failure(error, "commit append"))?;
            Ok(record)
        })
        .await
    }

    async fn head(&self, ceremony_id: &CeremonyId) -> Result<Option<AuditRecord>, DomainError> {
        Ok(self.records(ceremony_id).await?.pop())
    }

    async fn records(&self, ceremony_id: &CeremonyId) -> Result<Vec<AuditRecord>, DomainError> {
        let ceremony_id = ceremony_id.clone();
        self.blocking("records", move |database| {
            let read = database
                .begin_read()
                .map_err(|error| store_failure(error, "begin read"))?;
            let journal = read
                .open_table(JOURNAL)
                .map_err(|error| store_failure(error, "open journal"))?;
            journal_of(&journal, &ceremony_id)
        })
        .await
    }
}

#[async_trait]
impl OutboxPort for RedbCeremonyStore {
    /// Keys are grouped by ceremony and ordered within it, so one pass
    /// in key order visits each ceremony's queue in the order it was
    /// written. The first undelivered entry of a ceremony is its head,
    /// and it is the only one this claim can take: handing out two
    /// would put that ceremony's ordering in the publisher's hands.
    async fn claim(
        &self,
        limit: usize,
        now: OffsetDateTime,
        lease: DurationMs,
    ) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
        let lease_until = now + Duration::from_millis(lease.get());
        self.blocking("claim", move |database| {
            let write = database
                .begin_write()
                .map_err(|error| store_failure(error, "begin claim"))?;
            let claimed = {
                let mut outbox = write
                    .open_table(OUTBOX)
                    .map_err(|error| store_failure(error, "open outbox"))?;
                let entries = read_outbox(&outbox)?;

                let mut taken = Vec::new();
                let mut decided: Option<Vec<u8>> = None;
                for (key, stored) in entries {
                    let ceremony = ceremony_of(&key).unwrap_or_default().to_vec();
                    if decided.as_deref() == Some(ceremony.as_slice()) {
                        continue;
                    }
                    if stored.delivered {
                        continue;
                    }
                    // The head of this ceremony: claimable or not, it
                    // decides for the whole queue behind it.
                    decided = Some(ceremony);
                    if !stored.is_claimable(now) || taken.len() >= limit {
                        continue;
                    }
                    taken.push((key, stored));
                }

                let mut claimed = Vec::with_capacity(taken.len());
                for (key, mut stored) in taken {
                    stored.claimed_until = Some(lease_until);
                    outbox
                        .insert(
                            key.as_slice(),
                            encode(&stored, "encode outbox message")?.as_slice(),
                        )
                        .map_err(|error| store_failure(error, "record claim"))?;
                    claimed.push(ClaimedOutboxMessage::new(stored.message, stored.attempt));
                }
                claimed
            };
            write
                .commit()
                .map_err(|error| store_failure(error, "commit claim"))?;
            Ok(claimed)
        })
        .await
    }

    async fn mark_delivered(&self, event_ids: &[EventId]) -> Result<(), DomainError> {
        let event_ids = event_ids.to_vec();
        self.update_messages("mark_delivered", move |stored| {
            if event_ids.contains(stored.message.event_id()) {
                stored.delivered = true;
                stored.claimed_until = None;
                return true;
            }
            false
        })
        .await
    }

    async fn mark_failed(&self, event_id: &EventId) -> Result<(), DomainError> {
        let event_id = event_id.clone();
        self.update_messages("mark_failed", move |stored| {
            if stored.message.event_id() == &event_id {
                stored.attempt = stored.attempt.next();
                stored.claimed_until = None;
                return true;
            }
            false
        })
        .await
    }

    async fn quarantine(
        &self,
        event_id: &EventId,
        reason: OutboxQuarantineReason,
    ) -> Result<(), DomainError> {
        let event_id = event_id.clone();
        self.update_messages("quarantine", move |stored| {
            if stored.message.event_id() == &event_id {
                stored.quarantine = Some(reason.clone());
                stored.claimed_until = None;
                return true;
            }
            false
        })
        .await
    }

    async fn quarantined(&self) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
        self.blocking("quarantined", move |database| {
            let read = database
                .begin_read()
                .map_err(|error| store_failure(error, "begin read"))?;
            let outbox = read
                .open_table(OUTBOX)
                .map_err(|error| store_failure(error, "open outbox"))?;
            Ok(read_outbox(&outbox)?
                .into_iter()
                .filter(|(_, stored)| stored.quarantine.is_some())
                .map(|(_, stored)| ClaimedOutboxMessage::new(stored.message, stored.attempt))
                .collect())
        })
        .await
    }
}

impl RedbCeremonyStore {
    /// Apply `change` to every stored message it accepts, in one write
    /// transaction.
    async fn update_messages<F>(&self, op: &'static str, change: F) -> Result<(), DomainError>
    where
        F: Fn(&mut StoredOutboxMessage) -> bool + Send + 'static,
    {
        self.blocking(op, move |database| {
            let write = database
                .begin_write()
                .map_err(|error| store_failure(error, "begin update"))?;
            {
                let mut outbox = write
                    .open_table(OUTBOX)
                    .map_err(|error| store_failure(error, "open outbox"))?;
                let mut updates = Vec::new();
                for (key, mut stored) in read_outbox(&outbox)? {
                    if change(&mut stored) {
                        updates.push((key, stored));
                    }
                }
                for (key, stored) in updates {
                    outbox
                        .insert(
                            key.as_slice(),
                            encode(&stored, "encode outbox message")?.as_slice(),
                        )
                        .map_err(|error| store_failure(error, "update outbox message"))?;
                }
            }
            write
                .commit()
                .map_err(|error| store_failure(error, "commit update"))?;
            Ok(())
        })
        .await
    }
}

fn read_outbox(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
) -> Result<Vec<(Vec<u8>, StoredOutboxMessage)>, DomainError> {
    let mut entries = Vec::new();
    for entry in table
        .range::<&[u8]>(..)
        .map_err(|error| store_failure(error, "scan outbox"))?
    {
        let (key, value) = entry.map_err(|error| store_failure(error, "read outbox entry"))?;
        entries.push((
            key.value().to_vec(),
            decode(value.value(), "decode outbox message")?,
        ));
    }
    Ok(entries)
}

/// A published definition and the digest it was sealed with.
///
/// The digest is stored beside the definition rather than recomputed on
/// read: a stored definition whose recomputed digest disagrees with the
/// stored one is evidence the file was edited, and that is worth being
/// able to see.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPublication {
    definition: choreo_core::entities::CeremonyDefinition,
    digest: CeremonyDefinitionDigest,
}

impl StoredPublication {
    fn seal(published: &PublishedCeremonyDefinition) -> Self {
        Self {
            definition: published.definition().clone(),
            digest: published.digest(),
        }
    }

    fn restore(self) -> Result<PublishedCeremonyDefinition, DomainError> {
        PublishedCeremonyDefinition::seal(self.definition)
    }
}

#[async_trait]
impl CeremonyDefinitionPublicationPort for RedbCeremonyStore {
    /// The occupant is read and the slot written inside one write
    /// transaction, so two callers cannot publish different content
    /// under one version.
    async fn publish(
        &self,
        definition: PublishedCeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError> {
        self.blocking("publish", move |database| {
            let key = published(definition.name(), definition.version());
            let write = database
                .begin_write()
                .map_err(|error| store_failure(error, "begin publish"))?;
            let outcome = {
                let mut publications = write
                    .open_table(PUBLICATIONS)
                    .map_err(|error| store_failure(error, "open publications"))?;
                let occupant: Option<StoredPublication> = publications
                    .get(key.as_slice())
                    .map_err(|error| store_failure(error, "read publication"))?
                    .map(|value| decode(value.value(), "decode publication"))
                    .transpose()?;

                match occupant {
                    Some(occupant) if occupant.digest == definition.digest() => {
                        PublicationOutcome::AlreadyPublished(occupant.restore()?)
                    }
                    Some(occupant) => PublicationOutcome::VersionOccupied {
                        published: occupant.digest,
                        offered: definition.digest(),
                    },
                    None => {
                        publications
                            .insert(
                                key.as_slice(),
                                encode(
                                    &StoredPublication::seal(&definition),
                                    "encode publication",
                                )?
                                .as_slice(),
                            )
                            .map_err(|error| store_failure(error, "store publication"))?;
                        PublicationOutcome::Published(definition)
                    }
                }
            };

            if outcome.is_conflict() {
                return Ok(outcome);
            }
            write
                .commit()
                .map_err(|error| store_failure(error, "commit publish"))?;
            Ok(outcome)
        })
        .await
    }

    async fn published(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<Option<PublishedCeremonyDefinition>, DomainError> {
        let key = published(name, version);
        self.blocking("published", move |database| {
            let read = database
                .begin_read()
                .map_err(|error| store_failure(error, "begin read"))?;
            let publications = read
                .open_table(PUBLICATIONS)
                .map_err(|error| store_failure(error, "open publications"))?;
            let stored: Option<StoredPublication> = publications
                .get(key.as_slice())
                .map_err(|error| store_failure(error, "read publication"))?
                .map(|value| decode(value.value(), "decode publication"))
                .transpose()?;
            stored.map(StoredPublication::restore).transpose()
        })
        .await
    }

    async fn catalogue(&self) -> Result<Vec<PublishedCeremonyDefinition>, DomainError> {
        self.blocking("catalogue", move |database| {
            let read = database
                .begin_read()
                .map_err(|error| store_failure(error, "begin read"))?;
            let publications = read
                .open_table(PUBLICATIONS)
                .map_err(|error| store_failure(error, "open publications"))?;
            let mut catalogue = Vec::new();
            for entry in publications
                .range::<&[u8]>(..)
                .map_err(|error| store_failure(error, "scan publications"))?
            {
                let (_, value) =
                    entry.map_err(|error| store_failure(error, "read publication entry"))?;
                let stored: StoredPublication = decode(value.value(), "decode publication")?;
                catalogue.push(stored.restore()?);
            }
            Ok(catalogue)
        })
        .await
    }
}
