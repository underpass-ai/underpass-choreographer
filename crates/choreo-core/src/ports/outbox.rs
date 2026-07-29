//! [`OutboxPort`] and [`OutboxTransportPort`] — the two halves of
//! turning enqueued intent into delivered effect.
//!
//! The store holds messages that were committed with the state that
//! produced them. The transport takes one somewhere. Neither knows
//! about retries, ordering or exhaustion: that is the publisher's, so
//! a host does not reimplement it.

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::{
    ClaimedOutboxMessage, DurationMs, EventId, OutboxMessage, OutboxQuarantineReason,
};

#[async_trait]
pub trait OutboxPort: Send + Sync {
    /// Take up to `limit` messages for delivery, held for `lease`.
    ///
    /// `now` is passed rather than read: the same instant decides which
    /// existing claims have expired and when the new one does, and two
    /// clocks would let a message be claimed twice at the boundary.
    ///
    /// Claiming rather than reading is what lets two publishers run
    /// without both taking the same message; the lease is what lets a
    /// publisher die without stranding what it took. An expired claim
    /// is claimable again.
    ///
    /// **At most one message per ceremony.** Messages of one ceremony
    /// must arrive in the order they were committed, and handing out
    /// two at once would put that in the publisher's hands. Ceremonies
    /// are independent of each other, so nothing else is serialised.
    ///
    /// A quarantined message blocks its own ceremony and nothing else.
    /// Skipping past it would silently reorder that ceremony's stream,
    /// which is worse than a visible stall.
    async fn claim(
        &self,
        limit: usize,
        now: OffsetDateTime,
        lease: DurationMs,
    ) -> Result<Vec<ClaimedOutboxMessage>, DomainError>;

    /// Retire messages that reached their destination.
    ///
    /// Called only after the transport confirmed, never before: marking
    /// first would turn a failed publish into a lost message. Failing
    /// between the publish and this call produces a redelivery, which
    /// is why delivery is at-least-once and consumers key on the
    /// event id.
    async fn mark_delivered(&self, event_ids: &[EventId]) -> Result<(), DomainError>;

    /// Record that a message failed, so the next claim counts it.
    async fn mark_failed(&self, event_id: &EventId) -> Result<(), DomainError>;

    /// Stop retrying a message, with the reason on the record.
    async fn quarantine(
        &self,
        event_id: &EventId,
        reason: OutboxQuarantineReason,
    ) -> Result<(), DomainError>;

    /// Everything that stopped being retried.
    ///
    /// Quarantine is only defensible if it is visible; an unreadable
    /// dead-letter set is a silent discard with extra steps.
    async fn quarantined(&self) -> Result<Vec<ClaimedOutboxMessage>, DomainError>;
}

/// Where a message goes once it leaves the outbox.
///
/// Deliberately not the typed [`MessagingPort`](crate::ports::MessagingPort):
/// an outbox message is already serialized and addressed, and a host
/// that dispatches in process should not need a broker to satisfy this.
#[async_trait]
pub trait OutboxTransportPort: Send + Sync {
    async fn deliver(&self, message: &OutboxMessage) -> Result<(), DomainError>;
}
