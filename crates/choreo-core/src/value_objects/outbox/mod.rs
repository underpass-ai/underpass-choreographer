//! Value objects for the transactional outbox.

mod claimed_outbox_message;
mod outbox_attempt;
mod outbox_message;
mod outbox_quarantine_reason;
mod outbox_subject;

pub use claimed_outbox_message::ClaimedOutboxMessage;
pub use outbox_attempt::OutboxAttempt;
pub use outbox_message::OutboxMessage;
pub use outbox_quarantine_reason::OutboxQuarantineReason;
pub use outbox_subject::OutboxSubject;
