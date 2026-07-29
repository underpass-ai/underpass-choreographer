use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_REASON_LEN: usize = 512;

/// Why a message stopped being retried.
///
/// Required, not optional: a message removed from the queue without a
/// stated reason is a silent discard, which is the one outcome an
/// auditable engine cannot offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutboxQuarantineReason(String);

impl OutboxQuarantineReason {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "outbox_quarantine_reason",
            });
        }
        if trimmed.len() > MAX_REASON_LEN {
            return Err(DomainError::FieldTooLong {
                field: "outbox_quarantine_reason",
                actual: trimmed.len(),
                max: MAX_REASON_LEN,
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutboxQuarantineReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarantine_without_a_reason_is_rejected() {
        assert!(OutboxQuarantineReason::new("   ").is_err());
    }
}
