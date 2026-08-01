use std::fmt;
use std::sync::Arc;

use choreo_app::services::{SessionJournal, SessionMemoryRecorder};
use choreo_app::usecases::{
    ApplyCeremonyTransitionInput, ApplyCeremonyTransitionUseCase, ApproveCeremonyGuardInput,
    ApproveCeremonyGuardUseCase, AssertCeremonyReasonInput, AssertCeremonyReasonUseCase,
    BindCeremonyParticipantsInput, BindCeremonyParticipantsUseCase, CeremonyDefinitionSource,
    CloseCeremonyInterventionInput, CloseCeremonyInterventionUseCase, CollectCeremonyEvidenceInput,
    CollectCeremonyEvidenceUseCase, CompleteCeremonyStepInput, CompleteCeremonyStepUseCase,
    DeferCeremonyGuardInput, DeferCeremonyGuardUseCase, DiffCeremonyDefinitionsUseCase,
    GetCeremonyDefinitionUseCase, GetCeremonyInstanceUseCase, GetCeremonyTranscriptUseCase,
    ListCeremonyDefinitionsUseCase, ListCeremonyInstancesUseCase, MountCeremonyDefinitionsOutput,
    MountCeremonyDefinitionsUseCase, PublishCeremonyDefinitionUseCase,
    RequestCeremonyInterventionInput, RequestCeremonyInterventionUseCase,
    ResolveCeremonyDefinitionUseCase, RespondToCeremonyInterventionInput,
    RespondToCeremonyInterventionUseCase, RunCeremonyInput, RunCeremonyOutput,
    RunCeremonyStepInput, RunCeremonyStepOutput, RunCeremonyStepUseCase, RunCeremonyUseCase,
    StartCeremonyInput, StartCeremonyStepInput, StartCeremonyStepUseCase, StartCeremonyUseCase,
    StartPublishedCeremonyUseCase,
};
use choreo_core::entities::{
    CeremonyDefinition, CeremonyInstance, PublicationOutcome, PublishedCeremonyDefinition,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionPublicationPort, CeremonyDefinitionRepositoryPort,
    CeremonyEvidenceSourcePort, CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort,
    CeremonyTranscriptStorePort, CeremonyUnitOfWorkPort, ClockPort, MemoryWriterPort,
    MetricsRecorderPort,
};
use choreo_core::value_objects::{
    CeremonyDefinitionDiff, CeremonyId, CeremonyName, CeremonyTranscript, CeremonyVersion,
    StepAttempt,
};

use crate::{EmbeddedChoreographerBuilder, InProcessCeremonyDefinitionSource, VERSION};

/// In-process facade over the Choreographer ceremony use cases.
#[derive(Clone)]
pub struct EmbeddedChoreographer {
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    publications: Arc<dyn CeremonyDefinitionPublicationPort>,
    instances: Arc<dyn CeremonyInstanceRepositoryPort>,
    /// Reading a session with its revision, and storing it with the
    /// record of what it did.
    ///
    /// Kept alongside `instances` rather than replacing it: reads that
    /// change nothing have no revision to honour and no fact to seal,
    /// and routing them through here would only make them look
    /// transactional.
    journal: Arc<SessionJournal>,
    transcript_store: Arc<dyn CeremonyTranscriptStorePort>,
    step_handler: Arc<dyn CeremonyStepHandlerPort>,
    evidence_source: Arc<dyn CeremonyEvidenceSourcePort>,
    clock: Arc<dyn ClockPort>,
    metrics: Arc<dyn MetricsRecorderPort>,
    /// What a session leaves behind.
    ///
    /// A host that configures no memory gets one that forgets and says
    /// so, which is the honest shape of "not turned on". Handing it a
    /// kernel-backed writer instead is the whole of turning it on.
    session_memory: Arc<SessionMemoryRecorder>,
}

impl EmbeddedChoreographer {
    pub(crate) fn new(
        definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
        publications: Arc<dyn CeremonyDefinitionPublicationPort>,
        instances: Arc<dyn CeremonyInstanceRepositoryPort>,
        unit_of_work: Arc<dyn CeremonyUnitOfWorkPort>,
        transcript_store: Arc<dyn CeremonyTranscriptStorePort>,
        step_handler: Arc<dyn CeremonyStepHandlerPort>,
        evidence_source: Arc<dyn CeremonyEvidenceSourcePort>,
        clock: Arc<dyn ClockPort>,
        metrics: Arc<dyn MetricsRecorderPort>,
        memory: Arc<dyn MemoryWriterPort>,
    ) -> Self {
        Self {
            definitions,
            publications,
            journal: Arc::new(SessionJournal::new(unit_of_work, instances.clone())),
            instances,
            transcript_store,
            step_handler,
            evidence_source,
            clock,
            metrics,
            session_memory: Arc::new(SessionMemoryRecorder::new(memory)),
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

    pub async fn instances(&self) -> Result<Vec<CeremonyInstance>, DomainError> {
        ListCeremonyInstancesUseCase::new(self.instances.clone())
            .execute()
            .await
    }

    pub async fn transcript(&self, id: &CeremonyId) -> Result<CeremonyTranscript, DomainError> {
        GetCeremonyTranscriptUseCase::new(self.transcript_store.clone())
            .execute(id)
            .await
    }

    pub async fn run(&self, input: RunCeremonyInput) -> Result<RunCeremonyOutput, DomainError> {
        RunCeremonyUseCase::new(
            self.definitions.clone(),
            self.instances.clone(),
            self.step_handler.clone(),
            self.transcript_store.clone(),
            self.clock.clone(),
        )
        .with_metrics(self.metrics.clone())
        .execute(input)
        .await
    }

    /// Fix a definition to an immutable version.
    pub async fn publish_definition(
        &self,
        definition: CeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError> {
        PublishCeremonyDefinitionUseCase::new(self.publications.clone())
            .execute(definition)
            .await
    }

    /// The published definition under a name and version, if any.
    pub async fn published_definition(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<Option<PublishedCeremonyDefinition>, DomainError> {
        self.publications.published(name, version).await
    }

    /// Every published definition.
    pub async fn published_definitions(
        &self,
    ) -> Result<Vec<PublishedCeremonyDefinition>, DomainError> {
        self.publications.catalogue().await
    }

    /// The definition an instance actually runs, binding included.
    ///
    /// Delegates to the shared use case rather than holding the rule,
    /// so the embedded and deployable distributions cannot drift apart
    /// on what a bound instance means.
    pub async fn definition_for(
        &self,
        instance: &CeremonyInstance,
    ) -> Result<CeremonyDefinition, DomainError> {
        self.resolve_definition().execute(instance).await
    }

    /// How every verb that advances a session finds what it is running.
    /// A bound session resolves from the catalogue and is checked
    /// against the digest it recorded; an unbound one has only the
    /// repository. Handing this to the use cases is what lets a
    /// published session be advanced at all.
    fn resolve_definition(&self) -> Arc<ResolveCeremonyDefinitionUseCase> {
        Arc::new(ResolveCeremonyDefinitionUseCase::new(
            self.definitions.clone(),
            self.publications.clone(),
        ))
    }

    /// Seat this session's roles.
    pub async fn bind_participants(
        &self,
        input: BindCeremonyParticipantsInput,
    ) -> Result<CeremonyInstance, DomainError> {
        BindCeremonyParticipantsUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    /// Compare two definitions, either side published or supplied.
    pub async fn diff_definitions(
        &self,
        before: CeremonyDefinitionSource,
        after: CeremonyDefinitionSource,
    ) -> Result<CeremonyDefinitionDiff, DomainError> {
        DiffCeremonyDefinitionsUseCase::new(self.publications.clone())
            .execute(before, after)
            .await
    }

    /// Start an instance bound to a published definition's digest.
    pub async fn start_published(
        &self,
        input: StartCeremonyInput,
    ) -> Result<CeremonyInstance, DomainError> {
        StartPublishedCeremonyUseCase::new(
            self.publications.clone(),
            self.journal.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn start(&self, input: StartCeremonyInput) -> Result<CeremonyInstance, DomainError> {
        StartCeremonyUseCase::new(
            self.definitions.clone(),
            self.journal.clone(),
            self.clock.clone(),
        )
        .execute(input)
        .await
    }

    /// Say why one thing this session produced led to another.
    ///
    /// In-process only for now, and deliberately: a host embedding the
    /// engine can record its reasoning today without a wire format
    /// being settled for it.
    pub async fn assert_reason(
        &self,
        input: AssertCeremonyReasonInput,
    ) -> Result<CeremonyInstance, DomainError> {
        AssertCeremonyReasonUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
            self.clock.clone(),
            self.session_memory.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn approve_guard(
        &self,
        input: ApproveCeremonyGuardInput,
    ) -> Result<CeremonyInstance, DomainError> {
        ApproveCeremonyGuardUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
            self.clock.clone(),
            self.session_memory.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn defer_guard(
        &self,
        input: DeferCeremonyGuardInput,
    ) -> Result<CeremonyInstance, DomainError> {
        DeferCeremonyGuardUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
            self.clock.clone(),
            self.session_memory.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn request_intervention(
        &self,
        input: RequestCeremonyInterventionInput,
    ) -> Result<CeremonyInstance, DomainError> {
        RequestCeremonyInterventionUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
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
            self.resolve_definition(),
            self.journal.clone(),
            self.clock.clone(),
            self.session_memory.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn collect_evidence(
        &self,
        input: CollectCeremonyEvidenceInput,
    ) -> Result<CeremonyInstance, DomainError> {
        CollectCeremonyEvidenceUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
            self.evidence_source.clone(),
            self.clock.clone(),
            self.session_memory.clone(),
        )
        .execute(input)
        .await
    }

    pub async fn close_intervention(
        &self,
        input: CloseCeremonyInterventionInput,
    ) -> Result<CeremonyInstance, DomainError> {
        CloseCeremonyInterventionUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
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
            self.resolve_definition(),
            self.journal.clone(),
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
            self.resolve_definition(),
            self.journal.clone(),
            self.step_handler.clone(),
            self.clock.clone(),
        )
        .with_transcript_store(self.transcript_store.clone())
        .execute(input)
        .await
    }

    pub async fn complete_step(
        &self,
        input: CompleteCeremonyStepInput,
    ) -> Result<CeremonyInstance, DomainError> {
        CompleteCeremonyStepUseCase::new(
            self.resolve_definition(),
            self.journal.clone(),
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
            self.resolve_definition(),
            self.journal.clone(),
            self.clock.clone(),
            self.session_memory.clone(),
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
