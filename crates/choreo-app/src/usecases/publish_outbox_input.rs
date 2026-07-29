//! [`PublishOutboxInput`] — the delivery policy of one publishing round.

use choreo_core::error::DomainError;
use choreo_core::value_objects::DurationMs;

/// How much a round takes on, and when it gives up on a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishOutboxInput {
    batch_size: usize,
    max_attempts: u32,
    lease: DurationMs,
}

impl PublishOutboxInput {
    pub fn new(
        batch_size: usize,
        max_attempts: u32,
        lease: DurationMs,
    ) -> Result<Self, DomainError> {
        if batch_size == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "publish_outbox.batch_size",
            });
        }
        if max_attempts == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "publish_outbox.max_attempts",
            });
        }
        if lease.get() == 0 {
            return Err(DomainError::MustBeNonZero {
                field: "publish_outbox.lease",
            });
        }
        Ok(Self {
            batch_size,
            max_attempts,
            lease,
        })
    }

    #[must_use]
    pub fn batch_size(self) -> usize {
        self.batch_size
    }

    #[must_use]
    pub fn max_attempts(self) -> u32 {
        self.max_attempts
    }

    #[must_use]
    pub fn lease(self) -> DurationMs {
        self.lease
    }
}
