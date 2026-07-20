use std::fmt;
use std::sync::Arc;

use choreo_app::usecases::{
    ApplyCeremonyTransitionInput, ApplyCeremonyTransitionUseCase, ApproveCeremonyGuardInput,
    ApproveCeremonyGuardUseCase, CloseCeremonyInterventionInput, CloseCeremonyInterventionUseCase,
    CompleteCeremonyStepInput, CompleteCeremonyStepUseCase, GetCeremonyDefinitionUseCase,
    GetCeremonyInstanceUseCase, GetCeremonyTranscriptUseCase, ListCeremonyDefinitionsUseCase,
    MountCeremonyDefinitionsOutput, MountCeremonyDefinitionsUseCase,
    RequestCeremonyInterventionInput, RequestCeremonyInterventionUseCase,
    RespondToCeremonyInterventionInput, RespondToCeremonyInterventionUseCase, RunCeremonyInput,
    RunCeremonyOutput, RunCeremonyStepInput, RunCeremonyStepOutput, RunCeremonyStepUseCase,
    RunCeremonyUseCase, StartCeremonyInput, StartCeremonyStepInput, StartCeremonyStepUseCase,
    StartCeremonyUseCase,
};
use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyContextStorePort, CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort,
    CeremonyStepHandlerPort, ClockPort, MetricsRecorderPort,
};
use choreo_core::value_objects::{
    CeremonyId, CeremonyName, CeremonyTranscript, CeremonyVersion, StepAttempt,
};

use crate::{EmbeddedChoreographerBuilder, InProcessCeremonyDefinitionSource, VERSION};

/// In-process facade over the Choreographer ceremony use cases.
#[derive(Clone)]
pub struct EmbeddedChoreographer {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    context_store: Arc<dyn CeremonyContextStorePort>,
    step_handler: Arc<dyn CeremonyStepHandlerPort>,
    clock: Arc<dyn ClockPort>,
    metrics: Arc<dyn MetricsRecorderPort>,
}

impl EmbeddedChoreographer {
    pub(crate) fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        context_store: Arc<dyn CeremonyContextStorePort>,
        step_handler: Arc<dyn CeremonyStepHandlerPort>,
        clock: Arc<dyn ClockPort>,
        metrics: Arc<dyn MetricsRecorderPort>,
    ) -> Self {
        Self {
            definitions,
            instances,
            context_store,
            step_handler,
            clock,
            metrics,
        }
    }

    #[must_use]
    pub fn builder() -> EmbeddedChoreographerBuilder {
        EmbeddedChoreographerBuilder::new()
    }

    #[must_use]
    pub const fn version(&self) -> &'static str {
        VERSION
    }

    pub async fn mount_definition(
        &self,
        definition: CeremonyDefinition,
    ) -> Result<MountCeremonyDefinitionsOutput, DomainError> {
        self.mount_definitions([definition]).await
    }

    pub async fn mount_definitions(
        &self,
        definitions: impl IntoIterator<Item = CeremonyDefinition>,
    ) -> Result<MountCeremonyDefinitionsOutput, DomainError> {
        let source = Arc::new(InProcessCeremonyDefinitionSource::new(definitions));
        MountCeremonyDefinitionsUseCase::new(source, self.definitions.clone())
            .execute()
            .await
    }

    pub async fn mount_yaml(
        &self,
        raw: &str,
    ) -> Result<MountCeremonyDefinitionsOutput, DomainError> {
        let source = Arc::new(InProcessCeremonyDefinitionSource::from_yaml(raw)?);
        MountCeremonyDefinitionsUseCase::new(source, self.definitions.clone())
            .execute()
            .await
    }

    pub async fn definition(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<CeremonyDefinition, DomainError> {
        GetCeremonyDefinitionUseCase::new(self.definitions.clone())
            .execute(name, version)
            .await
    }

    pub async fn definitions(&self) -> Result<Vec<CeremonyDefinition>, DomainError> {
        ListCeremonyDefinitionsUseCase::new(self.definitions.clone())
            .execute()
            .await
    }

    pub async fn instance(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError> {
        GetCeremonyInstanceUseCase::new(self.instances.clone())
            .execute(id)
            .await
    }

    pub async fn transcript(&self, id: &CeremonyId) -> Result<CeremonyTranscript, DomainError> {
        GetCeremonyTranscriptUseCase::new(self.context_store.clone())
            .execute(id)
            .await
    }

    pub async fn run(&self, input: RunCeremonyInput) -> Result<RunCeremonyOutput, DomainError> {
        RunCeremonyUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.step_handler.clone(),
            self.context_store.clone(),
            self.clock.clone(),
        )
        .with_metrics(self.metrics.clone())
        .execute(input)
        .await
    }

    pub async fn start(&self, input: StartCeremonyInput) -> Result<CeremonyInstance, DomainError> {
        StartCeremonyUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn approve_guard(
        &self,
        input: ApproveCeremonyGuardInput,
    ) -> Result<CeremonyInstance, DomainError> {
        ApproveCeremonyGuardUseCase::new(self.instances.clone(), self.clock.clone())
            .execute(input)
            .await
    }

    pub async fn request_intervention(
        &self,
        input: RequestCeremonyInterventionInput,
    ) -> Result<CeremonyInstance, DomainError> {
        RequestCeremonyInterventionUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn respond_to_intervention(
        &self,
        input: RespondToCeremonyInterventionInput,
    ) -> Result<CeremonyInstance, DomainError> {
        RespondToCeremonyInterventionUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn close_intervention(
        &self,
        input: CloseCeremonyInterventionInput,
    ) -> Result<CeremonyInstance, DomainError> {
        CloseCeremonyInterventionUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn start_step(
        &self,
        input: StartCeremonyStepInput,
    ) -> Result<StepAttempt, DomainError> {
        StartCeremonyStepUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn run_step(
        &self,
        input: RunCeremonyStepInput,
    ) -> Result<RunCeremonyStepOutput, DomainError> {
        RunCeremonyStepUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.step_handler.clone(),
            self.clock.clone(),
        )
        .with_context_store(self.context_store.clone())
        .execute(input)
        .await
    }

    pub async fn complete_step(
        &self,
        input: CompleteCeremonyStepInput,
    ) -> Result<CeremonyInstance, DomainError> {
        CompleteCeremonyStepUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn apply_transition(
        &self,
        input: ApplyCeremonyTransitionInput,
    ) -> Result<CeremonyInstance, DomainError> {
        ApplyCeremonyTransitionUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }
}

impl Default for EmbeddedChoreographer {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl fmt::Debug for EmbeddedChoreographer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddedChoreographer")
            .field("version", &VERSION)
            .finish_non_exhaustive()
    }
}
