//! [`Council`] aggregate — a group of agents for a given specialty.
//!
//! The Council is an aggregate root that protects the invariants of
//! its agent membership. Callers interact with it through behavioural
//! methods (`add_agent`, `remove_agent`), not by mutating fields.
//!
//! Domain-agnostic port: the Python reference's `CouncilRegistry`
//! keyed councils by a SWE-specific `role` string. Here councils are
//! keyed by [`Specialty`] — free-form, operator-chosen.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::{AgentId, CouncilId, Specialty};

/// A council owns the set of agent identities that participate in
/// deliberations for its specialty. The membership must be non-empty
/// for the council to be capable of deliberating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Council {
    id: CouncilId,
    specialty: Specialty,
    agents: BTreeSet<AgentId>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

impl Council {
    /// Create a council seeded with at least one agent.
    ///
    /// A zero-agent council cannot deliberate; constructing one is
    /// rejected here rather than later in the use-case layer.
    pub fn new(
        id: CouncilId,
        specialty: Specialty,
        agents: impl IntoIterator<Item = AgentId>,
        now: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        let agents: BTreeSet<AgentId> = agents.into_iter().collect();
        if agents.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "council.agents",
            });
        }
        Ok(Self {
            id,
            specialty,
            agents,
            created_at: now,
        })
    }

    /// Add an agent. Idempotent: re-adding the same agent is a no-op,
    /// not an error.
    pub fn add_agent(&mut self, agent: AgentId) {
        self.agents.insert(agent);
    }

    /// Remove an agent. Rejects the removal if it would empty the
    /// council — an empty council cannot deliberate.
    pub fn remove_agent(&mut self, agent: &AgentId) -> Result<(), DomainError> {
        if self.agents.len() <= 1 && self.agents.contains(agent) {
            return Err(DomainError::InvariantViolated {
                reason: "council must retain at least one agent",
            });
        }
        if !self.agents.remove(agent) {
            return Err(DomainError::NotFound {
                what: "council.agent",
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn id(&self) -> &CouncilId {
        &self.id
    }
    #[must_use]
    pub fn specialty(&self) -> &Specialty {
        &self.specialty
    }
    #[must_use]
    pub fn agents(&self) -> &BTreeSet<AgentId> {
        &self.agents
    }
    #[must_use]
    pub fn size(&self) -> usize {
        self.agents.len()
    }
    #[must_use]
    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
    #[must_use]
    pub fn has_agent(&self, agent: &AgentId) -> bool {
        self.agents.contains(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn aid(s: &str) -> AgentId {
        AgentId::new(s).unwrap()
    }

    fn make(agents: Vec<AgentId>) -> Result<Council, DomainError> {
        Council::new(
            CouncilId::new("c1").unwrap(),
            Specialty::new("reviewer").unwrap(),
            agents,
            datetime!(2026-04-15 12:00:00 UTC),
        )
    }

    #[test]
    fn empty_council_is_rejected_at_construction() {
        assert!(matches!(
            make(vec![]).unwrap_err(),
            DomainError::EmptyCollection {
                field: "council.agents"
            }
        ));
    }

    #[test]
    fn construction_deduplicates_agents() {
        let c = make(vec![aid("a"), aid("a"), aid("b")]).unwrap();
        assert_eq!(c.size(), 2);
    }

    #[test]
    fn add_agent_is_idempotent() {
        let mut c = make(vec![aid("a")]).unwrap();
        c.add_agent(aid("b"));
        c.add_agent(aid("b"));
        assert_eq!(c.size(), 2);
    }

    #[test]
    fn remove_agent_requires_council_remains_non_empty() {
        let mut c = make(vec![aid("a")]).unwrap();
        let err = c.remove_agent(&aid("a")).unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
        assert_eq!(c.size(), 1);
    }

    #[test]
    fn remove_unknown_agent_reports_not_found() {
        let mut c = make(vec![aid("a"), aid("b")]).unwrap();
        assert!(matches!(
            c.remove_agent(&aid("missing")).unwrap_err(),
            DomainError::NotFound {
                what: "council.agent"
            }
        ));
    }

    #[test]
    fn remove_agent_shrinks_council_when_allowed() {
        let mut c = make(vec![aid("a"), aid("b")]).unwrap();
        c.remove_agent(&aid("a")).unwrap();
        assert_eq!(c.size(), 1);
        assert!(c.has_agent(&aid("b")));
        assert!(!c.has_agent(&aid("a")));
    }

    #[test]
    fn specialty_is_free_form_label() {
        // Regression: the council must not enumerate specialties.
        for label in ["triage", "planner", "clinical-intake", "sourcing"] {
            Council::new(
                CouncilId::new(label).unwrap(),
                Specialty::new(label).unwrap(),
                vec![aid("a")],
                datetime!(2026-04-15 12:00:00 UTC),
            )
            .unwrap();
        }
    }
}
