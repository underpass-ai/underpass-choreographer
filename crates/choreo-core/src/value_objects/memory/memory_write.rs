use crate::error::DomainError;

use super::{MemoryEntry, MemoryRelation};

/// What a session is handing to memory in one go.
///
/// Entries and their reasons travel together because they are one
/// thought. A write that landed the entries and lost the edges would
/// leave memory that reads correctly and cannot be followed, which is
/// the failure that looks least like a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryWrite {
    entries: Vec<MemoryEntry>,
    relations: Vec<MemoryRelation>,
}

impl MemoryWrite {
    /// Entries, and the reasons connecting them.
    ///
    /// A write with no entries is refused here rather than by each
    /// backend in turn: it is a call that would change nothing, and a
    /// backend answering "remembered" to it would be lying quietly.
    /// Refusing at construction means no backend is ever handed one.
    ///
    /// Relations may point outside this write. An ending explains a
    /// decision taken an hour ago, and whether that decision is still
    /// there is the backend's to say, not this type's — it cannot see
    /// what memory already holds.
    pub fn new(
        entries: Vec<MemoryEntry>,
        relations: Vec<MemoryRelation>,
    ) -> Result<Self, DomainError> {
        if entries.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "memory.entries",
            });
        }
        Ok(Self { entries, relations })
    }

    /// Entries with nothing yet connecting them.
    ///
    /// Honest for a first entry, which has nothing earlier to explain
    /// it. Reached for habitually, it is how a memory ends up being a
    /// list.
    pub fn unexplained(entries: Vec<MemoryEntry>) -> Result<Self, DomainError> {
        Self::new(entries, Vec::new())
    }

    #[must_use]
    pub fn entries(&self) -> &[MemoryEntry] {
        &self.entries
    }

    #[must_use]
    pub fn relations(&self) -> &[MemoryRelation] {
        &self.relations
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<MemoryEntry>, Vec<MemoryRelation>) {
        (self.entries, self.relations)
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;
    use crate::value_objects::{
        Attributes, CeremonyId, MemoryConfidence, MemoryEntryId, MemoryEntryKind, MemoryProvenance,
        MemoryRelationKind,
    };

    fn entry(id: &str) -> MemoryEntry {
        MemoryEntry::new(
            MemoryEntryId::new(id).expect("a valid id"),
            MemoryEntryKind::Decision,
            "something was settled",
            None,
            MemoryProvenance::new(
                CeremonyId::new("write-test").expect("a valid ceremony id"),
                None,
                OffsetDateTime::UNIX_EPOCH,
            ),
            Attributes::empty(),
        )
        .expect("a valid entry")
    }

    /// A call that would change nothing must not be answerable with
    /// "remembered". Refused here so no backend is ever handed one.
    #[test]
    fn a_write_with_no_entries_is_refused() {
        let outcome = MemoryWrite::new(Vec::new(), Vec::new());

        assert!(matches!(
            outcome,
            Err(DomainError::EmptyCollection {
                field: "memory.entries"
            })
        ));
    }

    #[test]
    fn a_write_may_explain_entries_written_earlier() {
        let relation = MemoryRelation::new(
            MemoryEntryId::new("today").expect("a valid id"),
            MemoryEntryId::new("last-week").expect("a valid id"),
            MemoryRelationKind::ChosenBecause,
            "the earlier finding is what settled this",
            MemoryConfidence::Medium,
        )
        .expect("a valid relation");

        let write = MemoryWrite::new(vec![entry("today")], vec![relation])
            .expect("relating to something absent is the backend's to judge");

        assert_eq!(write.relations().len(), 1);
    }

    #[test]
    fn entries_alone_are_a_write_without_reasons() {
        let write = MemoryWrite::unexplained(vec![entry("alone")]).expect("a valid write");

        assert!(write.relations().is_empty());
    }
}
