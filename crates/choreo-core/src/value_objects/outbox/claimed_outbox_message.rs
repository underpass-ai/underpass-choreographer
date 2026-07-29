use serde::{Deserialize, Serialize};

use super::{OutboxAttempt, OutboxMessage};

/// A message handed to a publisher, with what the store knows about how
/// its delivery has gone so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimedOutboxMessage {
    message: OutboxMessage,
    attempt: OutboxAttempt,
}

impl ClaimedOutboxMessage {
    #[must_use]
    pub fn new(message: OutboxMessage, attempt: OutboxAttempt) -> Self {
        Self { message, attempt }
    }

    #[must_use]
    pub fn message(&self) -> &OutboxMessage {
        &self.message
    }

    /// Attempts already made. The delivery about to be tried is not
    /// counted, so a publisher comparing this against its limit is
    /// asking "has this already failed too often?".
    #[must_use]
    pub fn attempt(&self) -> OutboxAttempt {
        self.attempt
    }
}
