use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_LENGTH: usize = 256;

/// What an entry is called, so something else can point at it.
///
/// Memory without names is a list. The moment one thing explains
/// another, both need to be nameable from outside the sentence that
/// relates them — and nameable again in a later session, hours after
/// the one that wrote them has ended.
///
/// So the name is the caller's to choose and the caller's to keep
/// stable. Deriving it here from content or from a counter would make
/// it unpredictable to the only party that could use it: the one
/// writing the entry that points back.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MemoryEntryId(String);

impl MemoryEntryId {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_entry_id",
            });
        }
        if trimmed.chars().count() > MAX_LENGTH {
            return Err(DomainError::FieldTooLong {
                field: "memory_entry_id",
                max: MAX_LENGTH,
                actual: trimmed.chars().count(),
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters {
                field: "memory_entry_id",
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MemoryEntryId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
