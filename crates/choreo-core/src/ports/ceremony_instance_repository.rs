//! [`CeremonyInstanceRepositoryPort`] — persistence for ceremony instances.

use async_trait::async_trait;

use crate::entities::CeremonyInstance;
use crate::error::DomainError;
use crate::value_objects::CeremonyId;

#[async_trait]
pub trait CeremonyInstanceRepositoryPort: Send + Sync {
    async fn save(&self, instance: &CeremonyInstance) -> Result<(), DomainError>;

    async fn get(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError>;

    async fn list(&self) -> Result<Vec<CeremonyInstance>, DomainError>;

    async fn exists(&self, id: &CeremonyId) -> Result<bool, DomainError>;
}
