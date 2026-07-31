use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::value_objects::Attributes;

use super::{MemoryDimension, MemoryEntryId, MemoryEntryKind, MemoryEvidence, MemoryProvenance};

const MAX_SUMMARY: usize = 2_048;

/// One thing worth remembering.
///
/// It carries a name, because an entry nothing can point at is a fact
/// with no place in an argument. What the name is belongs to whoever
/// writes it, and keeping it stable is what lets a later session
/// explain today's entry in terms of one written months ago.
///
/// The summary is capped, and deliberately short of anything that
/// could hold a conversation. What is remembered is what was decided
/// and why; the record of who said what is the transcript's job, and
/// a memory that swallowed transcripts would be a slower way to read
/// them rather than a way to navigate what they meant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    id: MemoryEntryId,
    kind: MemoryEntryKind,
    summary: String,
    dimension: Option<MemoryDimension>,
    provenance: MemoryProvenance,
    evidence: Vec<MemoryEvidence>,
    detail: Attributes,
}

impl MemoryEntry {
    pub fn new(
        id: MemoryEntryId,
        kind: MemoryEntryKind,
        summary: impl Into<String>,
        dimension: Option<MemoryDimension>,
        provenance: MemoryProvenance,
        detail: Attributes,
    ) -> Result<Self, DomainError> {
        let summary = summary.into();
        let trimmed = summary.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "memory_entry.summary",
            });
        }
        if trimmed.chars().count() > MAX_SUMMARY {
            return Err(DomainError::FieldTooLong {
                field: "memory_entry.summary",
                max: MAX_SUMMARY,
                actual: trimmed.chars().count(),
            });
        }
        Ok(Self {
            id,
            kind,
            summary: trimmed.to_owned(),
            dimension,
            provenance,
            evidence: Vec::new(),
            detail,
        })
    }

    #[must_use]
    pub fn with_evidence(mut self, evidence: impl IntoIterator<Item = MemoryEvidence>) -> Self {
        self.evidence.extend(evidence);
        self
    }

    /// What this entry is called, so a relation can point at it.
    #[must_use]
    pub fn id(&self) -> &MemoryEntryId {
        &self.id
    }

    #[must_use]
    pub const fn kind(&self) -> MemoryEntryKind {
        self.kind
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    #[must_use]
    pub fn dimension(&self) -> Option<&MemoryDimension> {
        self.dimension.as_ref()
    }

    #[must_use]
    pub fn provenance(&self) -> &MemoryProvenance {
        &self.provenance
    }

    #[must_use]
    pub fn evidence(&self) -> &[MemoryEvidence] {
        &self.evidence
    }

    #[must_use]
    pub fn detail(&self) -> &Attributes {
        &self.detail
    }
}
