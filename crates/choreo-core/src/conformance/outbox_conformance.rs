//! Conformance suite for [`OutboxPort`].
//!
//! Messages only exist because a commit put them there, so the suite
//! seeds through the unit of work rather than a back door: a store that
//! passed against fabricated rows would have been checked on a path
//! nothing uses.
//!
//! # What this suite cannot check
//!
//! **Publisher crash mid-delivery.** The lease properties show that a
//! claim expires and the message returns. Whether a store survives the
//! process holding that claim is the host's to prove.

use time::{Duration, OffsetDateTime};

use crate::error::DomainError;
use crate::ports::{CeremonyUnitOfWorkPort, OutboxPort};
use crate::value_objects::{
    CeremonyId, ClaimedOutboxMessage, DurationMs, ExpectedRevision, OutboxQuarantineReason,
};

use super::conformance_fixtures::commit_with;
use super::ConformanceFailure;

const LEASE_MS: u64 = 30_000;

/// Every property an [`OutboxPort`] implementation must satisfy.
#[derive(Debug)]
pub struct OutboxConformance;

impl OutboxConformance {
    /// Both ports must be backed by the same store: the outbox holds
    /// what the unit of work committed.
    pub async fn run(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<Vec<&'static str>, ConformanceFailure> {
        let mut passed = Vec::new();
        Self::an_empty_outbox_claims_nothing(outbox).await?;
        passed.push("an_empty_outbox_claims_nothing");
        Self::a_claim_yields_at_most_one_message_per_ceremony(outbox, unit_of_work).await?;
        passed.push("a_claim_yields_at_most_one_message_per_ceremony");
        Self::a_live_claim_is_not_handed_out_again(outbox, unit_of_work).await?;
        passed.push("a_live_claim_is_not_handed_out_again");
        Self::an_expired_claim_becomes_claimable(outbox, unit_of_work).await?;
        passed.push("an_expired_claim_becomes_claimable");
        Self::a_delivered_message_never_returns(outbox, unit_of_work).await?;
        passed.push("a_delivered_message_never_returns");
        Self::a_failure_advances_the_attempt_count(outbox, unit_of_work).await?;
        passed.push("a_failure_advances_the_attempt_count");
        Self::a_quarantined_message_blocks_only_its_own_ceremony(outbox, unit_of_work).await?;
        passed.push("a_quarantined_message_blocks_only_its_own_ceremony");
        Ok(passed)
    }

    async fn an_empty_outbox_claims_nothing(
        outbox: &dyn OutboxPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "an_empty_outbox_claims_nothing";

        let claimed = claim(PROPERTY, outbox, 8, now()).await?;
        if !claimed.is_empty() {
            return Err(failure(
                PROPERTY,
                format!("an unseeded outbox handed out {} messages", claimed.len()),
            ));
        }
        Ok(())
    }

    /// The ordering guarantee: a ceremony's stream cannot be reordered
    /// by a publisher holding two of its messages at once.
    async fn a_claim_yields_at_most_one_message_per_ceremony(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_claim_yields_at_most_one_message_per_ceremony";
        seed(PROPERTY, unit_of_work, "ordering", &["m1", "m2", "m3"]).await?;

        let claimed = claim(PROPERTY, outbox, 8, now()).await?;
        let from_this_ceremony = claimed
            .iter()
            .filter(|entry| entry.message().event_id().as_str().starts_with(PROPERTY))
            .count();

        if from_this_ceremony > 1 {
            return Err(failure(
                PROPERTY,
                format!("{from_this_ceremony} messages of one ceremony were claimed together"),
            ));
        }
        if from_this_ceremony == 0 {
            return Err(failure(PROPERTY, "a seeded ceremony yielded no message"));
        }
        Ok(())
    }

    async fn a_live_claim_is_not_handed_out_again(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_live_claim_is_not_handed_out_again";
        seed(PROPERTY, unit_of_work, "live", &["m1"]).await?;
        let moment = now();

        let first = mine(PROPERTY, claim(PROPERTY, outbox, 8, moment).await?);
        if first.is_empty() {
            return Err(failure(PROPERTY, "a seeded message was never claimable"));
        }

        let second = mine(PROPERTY, claim(PROPERTY, outbox, 8, moment).await?);
        if !second.is_empty() {
            return Err(failure(
                PROPERTY,
                "a message under a live claim was handed out again",
            ));
        }
        Ok(())
    }

    /// A publisher can die holding a claim. If the lease did not
    /// expire, that message would never be delivered by anyone.
    async fn an_expired_claim_becomes_claimable(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "an_expired_claim_becomes_claimable";
        seed(PROPERTY, unit_of_work, "expiring", &["m1"]).await?;
        let moment = now();

        claim(PROPERTY, outbox, 8, moment).await?;
        let after_expiry = mine(
            PROPERTY,
            claim(
                PROPERTY,
                outbox,
                8,
                moment + Duration::milliseconds(LEASE_MS as i64 + 1),
            )
            .await?,
        );

        if after_expiry.is_empty() {
            return Err(failure(
                PROPERTY,
                "a message whose lease expired stayed unclaimable — a dead publisher would strand it",
            ));
        }
        Ok(())
    }

    async fn a_delivered_message_never_returns(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_delivered_message_never_returns";
        seed(PROPERTY, unit_of_work, "delivered", &["m1", "m2"]).await?;

        let first = mine(PROPERTY, claim(PROPERTY, outbox, 8, now()).await?);
        let delivered = first
            .first()
            .ok_or_else(|| failure(PROPERTY, "nothing to deliver"))?
            .message()
            .event_id()
            .clone();
        call(
            PROPERTY,
            outbox
                .mark_delivered(std::slice::from_ref(&delivered))
                .await,
        )?;

        let next = mine(
            PROPERTY,
            claim(
                PROPERTY,
                outbox,
                8,
                now() + Duration::milliseconds(LEASE_MS as i64 + 1),
            )
            .await?,
        );
        if next
            .iter()
            .any(|entry| entry.message().event_id() == &delivered)
        {
            return Err(failure(
                PROPERTY,
                "a delivered message was handed out again",
            ));
        }
        if next.is_empty() {
            return Err(failure(
                PROPERTY,
                "delivering the head did not release the next message of the ceremony",
            ));
        }
        Ok(())
    }

    async fn a_failure_advances_the_attempt_count(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_failure_advances_the_attempt_count";
        seed(PROPERTY, unit_of_work, "failing", &["m1"]).await?;

        let first = mine(PROPERTY, claim(PROPERTY, outbox, 8, now()).await?);
        let entry = first
            .first()
            .ok_or_else(|| failure(PROPERTY, "nothing to fail"))?;
        if entry.attempt().value() != 0 {
            return Err(failure(
                PROPERTY,
                format!(
                    "a fresh message reported {} attempts",
                    entry.attempt().value()
                ),
            ));
        }
        let event_id = entry.message().event_id().clone();
        call(PROPERTY, outbox.mark_failed(&event_id).await)?;

        let second = mine(PROPERTY, claim(PROPERTY, outbox, 8, now()).await?);
        let retried = second
            .iter()
            .find(|entry| entry.message().event_id() == &event_id)
            .ok_or_else(|| failure(PROPERTY, "a failed message was not offered again"))?;
        if retried.attempt().value() != 1 {
            return Err(failure(
                PROPERTY,
                format!(
                    "after one failure the message reported {} attempts",
                    retried.attempt().value()
                ),
            ));
        }
        Ok(())
    }

    /// Quarantine stalls one stream visibly instead of reordering it
    /// silently, and leaves every other ceremony alone.
    async fn a_quarantined_message_blocks_only_its_own_ceremony(
        outbox: &dyn OutboxPort,
        unit_of_work: &dyn CeremonyUnitOfWorkPort,
    ) -> Result<(), ConformanceFailure> {
        const PROPERTY: &str = "a_quarantined_message_blocks_only_its_own_ceremony";
        seed(PROPERTY, unit_of_work, "blocked", &["m1", "m2"]).await?;
        seed(PROPERTY, unit_of_work, "neighbour", &["n1"]).await?;

        let claimed = mine(PROPERTY, claim(PROPERTY, outbox, 8, now()).await?);
        let poisoned = claimed
            .iter()
            .find(|entry| entry.message().event_id().as_str().contains("m1"))
            .ok_or_else(|| failure(PROPERTY, "the blocked ceremony yielded nothing"))?
            .message()
            .event_id()
            .clone();

        let reason = OutboxQuarantineReason::new("conformance").map_err(|error| {
            failure(
                PROPERTY,
                format!("the suite built an invalid reason: {error}"),
            )
        })?;
        call(PROPERTY, outbox.quarantine(&poisoned, reason).await)?;

        let later = mine(
            PROPERTY,
            claim(
                PROPERTY,
                outbox,
                8,
                now() + Duration::milliseconds(LEASE_MS as i64 + 1),
            )
            .await?,
        );
        if later
            .iter()
            .any(|entry| entry.message().event_id().as_str().contains("m2"))
        {
            return Err(failure(
                PROPERTY,
                "a message behind a quarantined one was handed out, silently reordering its ceremony",
            ));
        }
        if !later
            .iter()
            .any(|entry| entry.message().event_id().as_str().contains("n1"))
        {
            return Err(failure(
                PROPERTY,
                "one quarantined message stalled an unrelated ceremony",
            ));
        }

        let quarantined = call(PROPERTY, outbox.quarantined().await)?;
        if !quarantined
            .iter()
            .any(|entry| entry.message().event_id() == &poisoned)
        {
            return Err(failure(
                PROPERTY,
                "a quarantined message is not visible — that is a silent discard",
            ));
        }
        Ok(())
    }
}

fn now() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH
}

async fn claim(
    property: &'static str,
    outbox: &dyn OutboxPort,
    limit: usize,
    at: OffsetDateTime,
) -> Result<Vec<ClaimedOutboxMessage>, ConformanceFailure> {
    call(
        property,
        outbox
            .claim(limit, at, DurationMs::from_millis(LEASE_MS))
            .await,
    )
}

/// Only the messages this property seeded — a shared store may hold
/// others.
fn mine(property: &'static str, claimed: Vec<ClaimedOutboxMessage>) -> Vec<ClaimedOutboxMessage> {
    claimed
        .into_iter()
        .filter(|entry| entry.message().event_id().as_str().starts_with(property))
        .collect()
}

async fn seed(
    property: &'static str,
    unit_of_work: &dyn CeremonyUnitOfWorkPort,
    suffix: &str,
    messages: &[&str],
) -> Result<CeremonyId, ConformanceFailure> {
    let ceremony =
        CeremonyId::new(format!("conformance-{property}-{suffix}")).map_err(|error| {
            failure(
                property,
                format!("the suite built an invalid ceremony id: {error}"),
            )
        })?;
    let events = messages
        .iter()
        .map(|event| format!("{property}-{suffix}-{event}"))
        .collect::<Vec<_>>();
    let borrowed = events.iter().map(String::as_str).collect::<Vec<_>>();
    let commit = commit_with(
        &ceremony,
        ExpectedRevision::New,
        &format!("{property}-{suffix}-fact"),
        &borrowed,
    )
    .map_err(|error| {
        failure(
            property,
            format!("the suite built an invalid commit: {error}"),
        )
    })?;
    call(property, unit_of_work.commit(commit).await)?;
    Ok(ceremony)
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
