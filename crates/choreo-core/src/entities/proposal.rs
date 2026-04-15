//! [`Proposal`] entity.
//!
//! A proposal is a concrete solution artifact authored by an agent
//! during a deliberation. It has identity (`proposal_id`) and can be
//! revised in place: a revision replaces its content while keeping the
//! same identity — mirrors the Python reference implementation.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::{AgentId, Attributes, ProposalId, Specialty};

/// Authored proposal inside a deliberation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    id: ProposalId,
    author: AgentId,
    specialty: Specialty,
    content: String,
    attributes: Attributes,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    revision_count: u32,
}

impl Proposal {
    /// Create a new proposal.
    ///
    /// The content must not be empty.
    pub fn new(
        id: ProposalId,
        author: AgentId,
        specialty: Specialty,
        content: impl Into<String>,
        attributes: Attributes,
        now: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let content = content.into();
        if content.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "proposal.content",
            });
        }
        Ok(Self {
            id,
            author,
            specialty,
            content,
            attributes,
            created_at: now,
            updated_at: now,
            revision_count: 0,
        })
    }

    /// Replace the content (e.g. after peer-review revision).
    /// Increments the revision counter and refreshes `updated_at`.
    pub fn revise(
        &mut self,
        new_content: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        let content = new_content.into();
        if content.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "proposal.content",
            });
        }
        self.content = content;
        self.updated_at = now;
        self.revision_count = self.revision_count.saturating_add(1);
        Ok(())
    }

    #[must_use]
    pub fn id(&self) -> &ProposalId {
        &self.id
    }
    #[must_use]
    pub fn author(&self) -> &AgentId {
        &self.author
    }
    #[must_use]
    pub fn specialty(&self) -> &Specialty {
        &self.specialty
    }
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
    #[must_use]
    pub fn attributes(&self) -> &Attributes {
        &self.attributes
    }
    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
    #[must_use]
    pub fn updated_at(&self) -> OffsetDateTime {
        self.updated_at
    }
    #[must_use]
    pub fn revision_count(&self) -> u32 {
        self.revision_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn at() -> OffsetDateTime {
        datetime!(2026-04-15 12:00:00 UTC)
    }

    fn make(content: &str) -> Result<Proposal, DomainError> {
        Proposal::new(
            ProposalId::new("p1").unwrap(),
            AgentId::new("a1").unwrap(),
            Specialty::new("triage").unwrap(),
            content,
            Attributes::empty(),
            at(),
        )
    }

    #[test]
    fn new_requires_non_empty_content() {
        assert!(matches!(
            make("").unwrap_err(),
            DomainError::EmptyField {
                field: "proposal.content"
            }
        ));
        assert!(matches!(
            make("   \n").unwrap_err(),
            DomainError::EmptyField { .. }
        ));
    }

    #[test]
    fn new_seeds_timestamps_and_zero_revisions() {
        let p = make("plan A").unwrap();
        assert_eq!(p.created_at(), at());
        assert_eq!(p.updated_at(), at());
        assert_eq!(p.revision_count(), 0);
        assert_eq!(p.content(), "plan A");
    }

    #[test]
    fn revise_updates_content_and_bumps_revision() {
        let mut p = make("plan A").unwrap();
        let later = datetime!(2026-04-15 12:00:05 UTC);
        p.revise("plan A prime", later).unwrap();
        assert_eq!(p.content(), "plan A prime");
        assert_eq!(p.revision_count(), 1);
        assert_eq!(p.updated_at(), later);
        assert_eq!(p.created_at(), at()); // created_at unchanged
    }

    #[test]
    fn revise_preserves_identity() {
        let mut p = make("x").unwrap();
        let id_before = p.id().clone();
        p.revise("y", at()).unwrap();
        assert_eq!(p.id(), &id_before);
    }

    #[test]
    fn revise_rejects_empty_content() {
        let mut p = make("x").unwrap();
        assert!(p.revise("   ", at()).is_err());
        assert_eq!(p.content(), "x");
        assert_eq!(p.revision_count(), 0);
    }

    #[test]
    fn revision_count_saturates() {
        let mut p = make("x").unwrap();
        for _ in 0..3 {
            p.revise("y", at()).unwrap();
        }
        assert_eq!(p.revision_count(), 3);
    }
}
