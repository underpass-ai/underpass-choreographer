use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_LENGTH: usize = 128;

/// An axis within a scope.
///
/// One working session holds several strands at once — what each role
/// did, what happened in what order, what a given subsystem showed —
/// and an entry that names its strand can be found again by following
/// it. An entry with no dimension is still an entry; it is just harder
/// to come back to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MemoryDimension(String);

impl MemoryDimension {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_dimension",
            });
        }
        if trimmed.chars().count() > MAX_LENGTH {
            return Err(DomainError::FieldTooLong {
                field: "memory_dimension",
                max: MAX_LENGTH,
                actual: trimmed.chars().count(),
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters {
                field: "memory_dimension",
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
