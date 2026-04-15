//! [`DeliberationRepositoryPort`] — persistence for deliberations
//! across process restarts / replicas.

use async_trait::async_trait;

use crate::entities::Deliberation;
use crate::error::DomainError;
use crate::value_objects::TaskId;

#[async_trait]
pub trait DeliberationRepositoryPort: Send + Sync {
    /// Persist (or update) a deliberation keyed by its task id.
    async fn save(&self, deliberation: &Deliberation) -> Result<(), DomainError>;

    /// Fetch a deliberation by task id. Returns
    /// [`DomainError::NotFound`] when absent.
    async fn get(&self, task_id: &TaskId) -> Result<Deliberation, DomainError>;

    /// Cheap existence check.
    async fn exists(&self, task_id: &TaskId) -> Result<bool, DomainError>;
}
