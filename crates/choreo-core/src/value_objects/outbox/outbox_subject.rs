use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_SUBJECT_LEN: usize = 256;

/// Where an outbox message is destined once it is published.
///
/// Kept opaque to the engine: the domain decides that a message must go
/// out and under what name, and the transport decides what that name
/// means.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutboxSubject(String);

impl OutboxSubject {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "outbox_subject",
            });
        }
        if trimmed.len() > MAX_SUBJECT_LEN {
            return Err(DomainError::FieldTooLong {
                field: "outbox_subject",
                actual: trimmed.len(),
                max: MAX_SUBJECT_LEN,
            });
        }
        if trimmed.chars().any(char::is_whitespace) {
            return Err(DomainError::InvalidCharacters {
                field: "outbox_subject",
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutboxSubject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_spaced_subject_is_rejected() {
        assert!(OutboxSubject::new("  ").is_err());
        assert!(OutboxSubject::new("two words").is_err());
    }

    #[test]
    fn a_dotted_subject_is_accepted_and_trimmed() {
        let subject = OutboxSubject::new("  choreo.ceremony.completed  ").unwrap();

        assert_eq!(subject.as_str(), "choreo.ceremony.completed");
    }
}
