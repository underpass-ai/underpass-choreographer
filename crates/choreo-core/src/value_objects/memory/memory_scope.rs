use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::value_objects::CeremonyId;

const MAX_LENGTH: usize = 256;

/// What a memory is about.
///
/// A scope is the thing memory is organised around — for this engine a
/// working session, for a host that keeps memory of its own something
/// else. It is a validated string rather than a ceremony id because
/// the engine writing memory and the memory itself do not have to
/// agree on what the world is made of, only on how to name a corner
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MemoryScope(String);

impl MemoryScope {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_scope",
            });
        }
        if trimmed.chars().count() > MAX_LENGTH {
            return Err(DomainError::FieldTooLong {
                field: "memory_scope",
                max: MAX_LENGTH,
                actual: trimmed.chars().count(),
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters {
                field: "memory_scope",
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The scope a working session's memory belongs to.
    ///
    /// Prefixed rather than bare so a memory shared with other writers
    /// says what kind of thing it is about, and so a host that also
    /// keeps memory of its own — its cases, its tickets — can do that
    /// alongside without collision.
    pub fn of_ceremony(ceremony_id: &CeremonyId) -> Result<Self, DomainError> {
        Self::new(format!("ceremony:{}", ceremony_id.as_str()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
