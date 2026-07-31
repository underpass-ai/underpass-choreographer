use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::value_objects::Attributes;

const MAX_LABEL: usize = 256;

/// Something that backs an entry up.
///
/// Evidence is what turns a claim into one a reader can check. It is
/// kept beside the entry rather than inside it because the claim and
/// the proof of it are read at different times and by different
/// readers: a summary is skimmed, a proof is opened only when the
/// summary is doubted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvidence {
    label: String,
    source_id: Option<String>,
    detail: Attributes,
}

impl MemoryEvidence {
    pub fn new(
        label: impl Into<String>,
        source_id: Option<String>,
        detail: Attributes,
    ) -> Result<Self, DomainError> {
        let label = label.into();
        let trimmed = label.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_evidence.label",
            });
        }
        if trimmed.chars().count() > MAX_LABEL {
            return Err(DomainError::FieldTooLong {
                field: "memory_evidence.label",
                max: MAX_LABEL,
                actual: trimmed.chars().count(),
            });
        }
        Ok(Self {
            label: trimmed.to_owned(),
            source_id,
            detail,
        })
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Where it came from, when the engine knows. Absent evidence is
    /// still evidence — a person can attach what they saw without the
    /// engine having a name for the place they saw it.
    #[must_use]
    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    #[must_use]
    pub fn detail(&self) -> &Attributes {
        &self.detail
    }
}
