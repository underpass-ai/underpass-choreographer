use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_LENGTH: usize = 1_024;

/// A question put to memory in words.
///
/// Kept as its own type rather than a bare string so the cap is stated
/// once and so a reader of the port can tell a question from an
/// identifier at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuestion(String);

impl MemoryQuestion {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_question",
            });
        }
        if trimmed.chars().count() > MAX_LENGTH {
            return Err(DomainError::FieldTooLong {
                field: "memory_question",
                max: MAX_LENGTH,
                actual: trimmed.chars().count(),
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
