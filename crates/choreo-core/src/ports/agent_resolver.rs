//! [`AgentResolverPort`] — resolve agent identities into concrete
//! [`AgentPort`] instances.
//!
//! The [`Council`](crate::entities::Council) aggregate stores only
//! [`AgentId`]s to keep the domain pure of runtime objects. Use cases
//! that need to actually ask agents to propose / critique / revise
//! go through this port to obtain the live handles.
//!
//! Adapters typically back this port with an in-memory registry, a
//! factory that materializes agents from config, or a cache in front
//! of a remote agent service. The Choreographer itself never knows.

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::DomainError;
use crate::ports::agent::AgentPort;
use crate::value_objects::AgentId;

#[async_trait]
pub trait AgentResolverPort: Send + Sync {
    /// Resolve a single agent identity. Returns
    /// [`DomainError::NotFound`] when the id is unknown to the adapter.
    async fn resolve(&self, id: &AgentId) -> Result<Arc<dyn AgentPort>, DomainError>;

    /// Resolve a batch of agent identities preserving input order.
    /// Returns [`DomainError::NotFound`] on the first unresolved id.
    ///
    /// Default implementation calls [`Self::resolve`] in order; adapters
    /// can override to batch lookups.
    async fn resolve_all(&self, ids: &[AgentId]) -> Result<Vec<Arc<dyn AgentPort>>, DomainError> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.resolve(id).await?);
        }
        Ok(out)
    }
}
