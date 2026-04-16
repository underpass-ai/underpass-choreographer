//! No-op [`AgentPort`].
//!
//! Returns deterministic, self-describing content so the full
//! deliberation pipeline can run without an LLM or any other
//! provider. Intended for tests and minimal deployments; it must not
//! be confused with a real agent.

use async_trait::async_trait;
use choreo_core::entities::TaskConstraints;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentPort, Critique, DraftRequest, Revision};
use choreo_core::value_objects::{AgentId, Specialty};

/// A deterministic agent that composes its own `id` and the task
/// description into its proposal / critique / revision outputs. Safe
/// to run without any external service.
#[derive(Debug, Clone)]
pub struct NoopAgent {
    id: AgentId,
    specialty: Specialty,
}

impl NoopAgent {
    #[must_use]
    pub fn new(id: AgentId, specialty: Specialty) -> Self {
        Self { id, specialty }
    }
}

#[async_trait]
impl AgentPort for NoopAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn specialty(&self) -> &Specialty {
        &self.specialty
    }

    async fn generate(&self, request: DraftRequest) -> Result<Revision, DomainError> {
        Ok(Revision {
            content: format!(
                "noop-proposal[{id}] for task: {task}",
                id = self.id,
                task = request.task.as_str(),
            ),
        })
    }

    async fn critique(
        &self,
        peer_content: &str,
        _constraints: &TaskConstraints,
    ) -> Result<Critique, DomainError> {
        Ok(Critique {
            feedback: format!(
                "noop-critique[{id}] on peer content of length {n}",
                id = self.id,
                n = peer_content.len(),
            ),
        })
    }

    async fn revise(
        &self,
        own_content: &str,
        _critique: &Critique,
    ) -> Result<Revision, DomainError> {
        Ok(Revision {
            content: format!("{own_content}[revised-by:{id}]", id = self.id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::TaskDescription;

    fn agent(id: &str) -> NoopAgent {
        NoopAgent::new(AgentId::new(id).unwrap(), Specialty::new("triage").unwrap())
    }

    #[tokio::test]
    async fn generate_is_deterministic_and_includes_identity() {
        let a = agent("a1");
        let out = a
            .generate(DraftRequest {
                task: TaskDescription::new("hello").unwrap(),
                constraints: TaskConstraints::default(),
                diverse: true,
            })
            .await
            .unwrap();
        assert!(out.content.contains("a1"));
        assert!(out.content.contains("hello"));
    }

    #[tokio::test]
    async fn critique_mentions_reviewer_id() {
        let a = agent("a1");
        let out = a
            .critique("abc", &TaskConstraints::default())
            .await
            .unwrap();
        assert!(out.feedback.contains("a1"));
    }

    #[tokio::test]
    async fn revise_appends_marker_preserving_own_content() {
        let a = agent("a1");
        let out = a
            .revise(
                "seed-content",
                &Critique {
                    feedback: String::new(),
                },
            )
            .await
            .unwrap();
        assert!(out.content.starts_with("seed-content"));
        assert!(out.content.contains("a1"));
    }

    #[test]
    fn specialty_is_returned_as_given() {
        let a = NoopAgent::new(
            AgentId::new("a").unwrap(),
            Specialty::new("triage").unwrap(),
        );
        assert_eq!(a.specialty().as_str(), "triage");
    }
}
