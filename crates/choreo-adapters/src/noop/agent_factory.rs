//! No-op [`AgentFactoryPort`] — materializes [`NoopAgent`]s for the
//! `"noop"` kind and rejects every other kind with
//! [`DomainError::InvariantViolated`].
//!
//! The composition root wires this factory unconditionally so a fresh
//! deployment can exercise `RegisterAgent` without any provider feature
//! enabled. Provider-backed factories (vLLM, Anthropic, OpenAI, …) can
//! be composed alongside this one via a dispatching factory when those
//! slices land; this adapter owns only `kind == "noop"`.

use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{AgentDescriptor, AgentFactoryPort, AgentPort};

use super::agent::NoopAgent;

pub const NOOP_AGENT_KIND: &str = "noop";

#[derive(Debug, Default, Clone)]
pub struct NoopAgentFactory;

impl NoopAgentFactory {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentFactoryPort for NoopAgentFactory {
    async fn create(&self, descriptor: AgentDescriptor) -> Result<Arc<dyn AgentPort>, DomainError> {
        if descriptor.kind.as_str() != NOOP_AGENT_KIND {
            return Err(DomainError::InvariantViolated {
                reason: "noop agent factory only accepts kind=\"noop\"",
            });
        }
        Ok(Arc::new(NoopAgent::new(
            descriptor.id,
            descriptor.specialty,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{AgentId, AgentKind, Attributes, Specialty};

    fn descriptor(id: &str, kind: &str) -> AgentDescriptor {
        AgentDescriptor {
            id: AgentId::new(id).unwrap(),
            specialty: Specialty::new("triage").unwrap(),
            kind: AgentKind::new(kind).unwrap(),
            attributes: Attributes::empty(),
        }
    }

    #[tokio::test]
    async fn noop_kind_is_materialized() {
        let factory = NoopAgentFactory::new();
        let agent = factory.create(descriptor("a1", "noop")).await.unwrap();
        assert_eq!(agent.id().as_str(), "a1");
        assert_eq!(agent.specialty().as_str(), "triage");
    }

    #[tokio::test]
    async fn unsupported_kind_is_rejected_with_invariant_violation() {
        let factory = NoopAgentFactory::new();
        let err = factory.create(descriptor("a1", "vllm")).await.unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }
}
