use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::error::DomainError;
use choreo_core::ports::{CeremonyStepHandlerPort, CeremonyStepHandlerRequest};
use choreo_core::value_objects::StepResult;

type StepHandlerFuture =
    Pin<Box<dyn Future<Output = Result<StepResult, DomainError>> + Send + 'static>>;
type StepHandlerCallback =
    dyn Fn(CeremonyStepHandlerRequest) -> StepHandlerFuture + Send + Sync + 'static;

/// Adapts an async host callback to [`CeremonyStepHandlerPort`].
///
/// The host receives a typed step request and returns a typed step result
/// without implementing a transport or running a Choreographer service.
#[derive(Clone)]
pub struct CallbackCeremonyStepHandler {
    callback: Arc<StepHandlerCallback>,
}

impl CallbackCeremonyStepHandler {
    #[must_use]
    pub fn new<F, Fut>(callback: F) -> Self
    where
        F: Fn(CeremonyStepHandlerRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<StepResult, DomainError>> + Send + 'static,
    {
        Self {
            callback: Arc::new(move |request| Box::pin(callback(request))),
        }
    }
}

impl fmt::Debug for CallbackCeremonyStepHandler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CallbackCeremonyStepHandler")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CeremonyStepHandlerPort for CallbackCeremonyStepHandler {
    async fn execute(
        &self,
        request: CeremonyStepHandlerRequest,
    ) -> Result<StepResult, DomainError> {
        (self.callback)(request).await
    }
}
