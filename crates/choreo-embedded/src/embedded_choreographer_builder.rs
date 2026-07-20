use std::fmt;
use std::future::Future;
use std::sync::Arc;

use choreo_adapters::clock::SystemClock;
use choreo_adapters::memory::{
    InMemoryCeremonyContextStore, InMemoryCeremonyDefinitionRepository,
    InMemoryCeremonyInstanceRepository,
};
use choreo_adapters::noop::NoopCeremonyStepHandler;
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyContextStorePort, CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort,
    CeremonyStepHandlerPort, CeremonyStepHandlerRequest, ClockPort, MetricsRecorderPort,
    NoopMetricsRecorder,
};
use choreo_core::value_objects::StepResult;

use crate::{CallbackCeremonyStepHandler, EmbeddedChoreographer};

/// Builder for an in-process Choreographer with replaceable adapters.
#[derive(Default)]
pub struct EmbeddedChoreographerBuilder {
    definitions: Option<Arc<dyn CeremonyDefinitionRepositoryPort>>,
    instances: Option<Arc<dyn CeremonyInstanceRepositoryPort>>,
    context_store: Option<Arc<dyn CeremonyContextStorePort>>,
    step_handler: Option<Arc<dyn CeremonyStepHandlerPort>>,
    clock: Option<Arc<dyn ClockPort>>,
    metrics: Option<Arc<dyn MetricsRecorderPort>>,
}

impl EmbeddedChoreographerBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_definition_repository(
        mut self,
        adapter: Arc<dyn CeremonyDefinitionRepositoryPort>,
    ) -> Self {
        self.definitions = Some(adapter);
        self
    }

    #[must_use]
    pub fn with_instance_repository(
        mut self,
        adapter: Arc<dyn CeremonyInstanceRepositoryPort>,
    ) -> Self {
        self.instances = Some(adapter);
        self
    }

    #[must_use]
    pub fn with_context_store(mut self, adapter: Arc<dyn CeremonyContextStorePort>) -> Self {
        self.context_store = Some(adapter);
        self
    }

    #[must_use]
    pub fn with_step_handler(mut self, adapter: Arc<dyn CeremonyStepHandlerPort>) -> Self {
        self.step_handler = Some(adapter);
        self
    }

    #[must_use]
    pub fn with_step_handler_callback<F, Fut>(self, callback: F) -> Self
    where
        F: Fn(CeremonyStepHandlerRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<StepResult, DomainError>> + Send + 'static,
    {
        self.with_step_handler(Arc::new(CallbackCeremonyStepHandler::new(callback)))
    }

    #[must_use]
    pub fn with_clock(mut self, adapter: Arc<dyn ClockPort>) -> Self {
        self.clock = Some(adapter);
        self
    }

    #[must_use]
    pub fn with_metrics(mut self, adapter: Arc<dyn MetricsRecorderPort>) -> Self {
        self.metrics = Some(adapter);
        self
    }

    /// Build with in-memory, side-effect-free defaults for every adapter not
    /// supplied by the host.
    #[must_use]
    pub fn build(self) -> EmbeddedChoreographer {
        let definitions = self.definitions.unwrap_or_else(|| {
            Arc::new(InMemoryCeremonyDefinitionRepository::new())
                as Arc<dyn CeremonyDefinitionRepositoryPort>
        });
        let instances = self.instances.unwrap_or_else(|| {
            Arc::new(InMemoryCeremonyInstanceRepository::new())
                as Arc<dyn CeremonyInstanceRepositoryPort>
        });
        let context_store = self.context_store.unwrap_or_else(|| {
            Arc::new(InMemoryCeremonyContextStore::new()) as Arc<dyn CeremonyContextStorePort>
        });
        let step_handler = self.step_handler.unwrap_or_else(|| {
            Arc::new(NoopCeremonyStepHandler::new()) as Arc<dyn CeremonyStepHandlerPort>
        });
        let clock = self
            .clock
            .unwrap_or_else(|| Arc::new(SystemClock::new()) as Arc<dyn ClockPort>);
        let metrics = self
            .metrics
            .unwrap_or_else(|| Arc::new(NoopMetricsRecorder) as Arc<dyn MetricsRecorderPort>);

        EmbeddedChoreographer::new(
            definitions,
            instances,
            context_store,
            step_handler,
            clock,
            metrics,
        )
    }
}

impl fmt::Debug for EmbeddedChoreographerBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddedChoreographerBuilder")
            .field("has_definition_repository", &self.definitions.is_some())
            .field("has_instance_repository", &self.instances.is_some())
            .field("has_context_store", &self.context_store.is_some())
            .field("has_step_handler", &self.step_handler.is_some())
            .field("has_clock", &self.clock.is_some())
            .field("has_metrics", &self.metrics.is_some())
            .finish()
    }
}
