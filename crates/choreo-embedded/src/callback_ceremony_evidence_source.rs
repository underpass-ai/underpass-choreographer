use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::CeremonyEvidencePack;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyEvidenceRequest, CeremonyEvidenceSourcePort};

type EvidenceSourceFuture =
    Pin<Box<dyn Future<Output = Result<CeremonyEvidencePack, DomainError>> + Send + 'static>>;
type EvidenceSourceCallback =
    dyn Fn(CeremonyEvidenceRequest) -> EvidenceSourceFuture + Send + Sync + 'static;

/// Adapts an async host callback to [`CeremonyEvidenceSourcePort`].
#[derive(Clone)]
pub struct CallbackCeremonyEvidenceSource {
    callback: Arc<EvidenceSourceCallback>,
}

impl CallbackCeremonyEvidenceSource {
    #[must_use]
    pub fn new<F, Fut>(callback: F) -> Self
    where
        F: Fn(CeremonyEvidenceRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<CeremonyEvidencePack, DomainError>> + Send + 'static,
    {
        Self {
            callback: Arc::new(move |request| Box::pin(callback(request))),
        }
    }
}

impl fmt::Debug for CallbackCeremonyEvidenceSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CallbackCeremonyEvidenceSource")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CeremonyEvidenceSourcePort for CallbackCeremonyEvidenceSource {
    async fn collect(
        &self,
        request: CeremonyEvidenceRequest,
    ) -> Result<CeremonyEvidencePack, DomainError> {
        (self.callback)(request).await
    }
}
