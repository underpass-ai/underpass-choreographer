//! Value objects for the transactional outbox.

mod outbox_message;
mod outbox_subject;

pub use outbox_message::OutboxMessage;
pub use outbox_subject::OutboxSubject;
