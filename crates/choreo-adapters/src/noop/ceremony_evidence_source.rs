use async_trait::async_trait;
use choreo_core::entities::CeremonyEvidencePack;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyEvidenceRequest, CeremonyEvidenceSourcePort};

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopCeremonyEvidenceSource;

impl NoopCeremonyEvidenceSource {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CeremonyEvidenceSourcePort for NoopCeremonyEvidenceSource {
    async fn collect(
        &self,
        _request: CeremonyEvidenceRequest,
    ) -> Result<CeremonyEvidencePack, DomainError> {
        Err(DomainError::NotFound {
            what: "ceremony_evidence_source",
        })
    }
}
