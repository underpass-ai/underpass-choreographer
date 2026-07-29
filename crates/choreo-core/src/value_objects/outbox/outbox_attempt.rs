use serde::{Deserialize, Serialize};

/// How many times delivery of a message has been tried.
///
/// Counted by the store rather than carried by the message: a message
/// states what happened in the ceremony, and how hard it has been to
/// publish is not part of that.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct OutboxAttempt(u32);

impl OutboxAttempt {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    #[must_use]
    pub fn is_exhausted(self, max_attempts: u32) -> bool {
        self.0 >= max_attempts
    }
}
