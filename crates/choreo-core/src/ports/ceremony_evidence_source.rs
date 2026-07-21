use async_trait::async_trait;

use crate::entities::CeremonyEvidencePack;
use crate::error::DomainError;

use super::CeremonyEvidenceRequest;

#[async_trait]
pub trait CeremonyEvidenceSourcePort: Send + Sync {
    async fn collect(
        &self,
        request: CeremonyEvidenceRequest,
    ) -> Result<CeremonyEvidencePack, DomainError>;
}
