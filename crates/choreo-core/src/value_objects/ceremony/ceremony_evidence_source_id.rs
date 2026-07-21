use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_ID_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CeremonyEvidenceSourceId(String);

impl CeremonyEvidenceSourceId {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "ceremony_evidence_source_id",
            });
        }
        if trimmed.len() > MAX_ID_LEN {
            return Err(DomainError::FieldTooLong {
                field: "ceremony_evidence_source_id",
                actual: trimmed.len(),
                max: MAX_ID_LEN,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_evidence_source_id",
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CeremonyEvidenceSourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_validates_source_identity() {
        let id = CeremonyEvidenceSourceId::new("  observability  ").unwrap();

        assert_eq!(id.as_str(), "observability");
        assert!(CeremonyEvidenceSourceId::new(" ").is_err());
    }
}
