use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use choreo_core::entities::{
    CeremonyDefinition, CeremonyInstance, PublicationOutcome, PublishedCeremonyDefinition,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionPublicationPort, CeremonyDefinitionRepositoryPort,
    CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort, CeremonyStepHandlerRequest,
    CeremonyTranscriptStorePort, ClockPort, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    CeremonyContext, CeremonyGuard, CeremonyId, CeremonyName, CeremonyRole, CeremonyState,
    CeremonyStep, CeremonyStepContribution, CeremonyTranscript, CeremonyTransition,
    CeremonyVersion, DurationMs, GuardCondition, GuardName, IdempotencyKey, LeaseOwnerId,
    MemoryCapabilities, MemoryCapability, MemoryEntry, MemoryRelation, MemoryScope, MemoryWrite,
    RetryPolicy, RoleAction, RoleId, StateId, StepAttempt, StepHandlerConfig, StepHandlerKind,
    StepId, StepResult, StepStatus, TransitionTrigger,
};
use time::macros::datetime;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use super::resolve_ceremony_definition_use_case::ResolveCeremonyDefinitionUseCase;
use crate::services::SessionMemoryRecorder;

#[derive(Debug, Clone, Copy)]
pub(super) struct FixedClock {
    now: OffsetDateTime,
}

impl FixedClock {
    pub(super) fn new(now: OffsetDateTime) -> Self {
        Self { now }
    }
}

impl ClockPort for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.now
    }
}

#[derive(Debug, Default)]
pub(super) struct DefinitionRepositoryFake {
    inner: RwLock<BTreeMap<(CeremonyName, CeremonyVersion), CeremonyDefinition>>,
}

impl DefinitionRepositoryFake {
    pub(super) fn new(definition: CeremonyDefinition) -> Self {
        let mut inner = BTreeMap::new();
        inner.insert(
            (definition.name().clone(), definition.version().clone()),
            definition,
        );
        Self {
            inner: RwLock::new(inner),
        }
    }
}

#[async_trait]
impl CeremonyDefinitionRepositoryPort for DefinitionRepositoryFake {
    async fn save(&self, definition: &CeremonyDefinition) -> Result<(), DomainError> {
        self.inner.write().await.insert(
            (definition.name().clone(), definition.version().clone()),
            definition.clone(),
        );
        Ok(())
    }

    async fn get(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<CeremonyDefinition, DomainError> {
        self.inner
            .read()
            .await
            .get(&(name.clone(), version.clone()))
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_definition",
            })
    }

    async fn list(&self) -> Result<Vec<CeremonyDefinition>, DomainError> {
        Ok(self.inner.read().await.values().cloned().collect())
    }
}

#[derive(Debug, Default)]
pub(super) struct InstanceRepositoryFake {
    inner: RwLock<BTreeMap<CeremonyId, CeremonyInstance>>,
}

impl InstanceRepositoryFake {
    pub(super) async fn saved(&self, id: &CeremonyId) -> CeremonyInstance {
        self.get(id).await.unwrap()
    }
}

#[async_trait]
impl CeremonyInstanceRepositoryPort for InstanceRepositoryFake {
    async fn save(&self, instance: &CeremonyInstance) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .insert(instance.id().clone(), instance.clone());
        Ok(())
    }

    async fn get(&self, id: &CeremonyId) -> Result<CeremonyInstance, DomainError> {
        self.inner
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or(DomainError::NotFound {
                what: "ceremony_instance",
            })
    }

    async fn list(&self) -> Result<Vec<CeremonyInstance>, DomainError> {
        Ok(self.inner.read().await.values().cloned().collect())
    }

    async fn exists(&self, id: &CeremonyId) -> Result<bool, DomainError> {
        Ok(self.inner.read().await.contains_key(id))
    }
}

#[derive(Debug)]
pub(super) struct StepHandlerFake {
    result: Result<StepResult, DomainError>,
    requests: RwLock<Vec<CeremonyStepHandlerRequest>>,
}

impl StepHandlerFake {
    pub(super) fn succeeding(result: StepResult) -> Self {
        Self {
            result: Ok(result),
            requests: RwLock::new(Vec::new()),
        }
    }

    pub(super) fn failing(error: DomainError) -> Self {
        Self {
            result: Err(error),
            requests: RwLock::new(Vec::new()),
        }
    }

    pub(super) async fn requests(&self) -> Vec<CeremonyStepHandlerRequest> {
        self.requests.read().await.clone()
    }
}

#[async_trait]
impl CeremonyStepHandlerPort for StepHandlerFake {
    async fn execute(
        &self,
        request: CeremonyStepHandlerRequest,
    ) -> Result<StepResult, DomainError> {
        self.requests.write().await.push(request);
        self.result.clone()
    }
}

#[derive(Debug, Default)]
pub(super) struct ContextStoreFake {
    inner: RwLock<BTreeMap<CeremonyId, Vec<CeremonyStepContribution>>>,
}

#[async_trait]
impl CeremonyTranscriptStorePort for ContextStoreFake {
    async fn append(
        &self,
        instance_id: &CeremonyId,
        contribution: CeremonyStepContribution,
    ) -> Result<(), DomainError> {
        self.inner
            .write()
            .await
            .entry(instance_id.clone())
            .or_default()
            .push(contribution);
        Ok(())
    }

    async fn transcript(
        &self,
        instance_id: &CeremonyId,
    ) -> Result<CeremonyTranscript, DomainError> {
        Ok(CeremonyTranscript::new(
            self.inner
                .read()
                .await
                .get(instance_id)
                .cloned()
                .unwrap_or_default(),
        ))
    }
}

pub(super) fn now() -> OffsetDateTime {
    datetime!(2026-06-06 12:00:00 UTC)
}

pub(super) fn definition_name() -> CeremonyName {
    CeremonyName::new("editorial_meeting").unwrap()
}

pub(super) fn approval_definition_name() -> CeremonyName {
    CeremonyName::new("approval_ceremony").unwrap()
}

pub(super) fn version() -> CeremonyVersion {
    CeremonyVersion::v1()
}

pub(super) fn ceremony_id() -> CeremonyId {
    CeremonyId::new("ceremony-1").unwrap()
}

pub(super) fn role_id() -> RoleId {
    RoleId::new("FACILITATOR").unwrap()
}

pub(super) fn respondent_role_id() -> RoleId {
    RoleId::new("TABLE_MEMBER").unwrap()
}

pub(super) fn step_id() -> StepId {
    StepId::new("roundtable").unwrap()
}

pub(super) fn trigger() -> TransitionTrigger {
    TransitionTrigger::new("meeting_done").unwrap()
}

pub(super) fn lease_owner() -> LeaseOwnerId {
    LeaseOwnerId::new("runner-1").unwrap()
}

pub(super) fn idempotency_key(value: &str) -> IdempotencyKey {
    IdempotencyKey::new(value).unwrap()
}

pub(super) fn lease_ttl() -> DurationMs {
    DurationMs::from_millis(60_000)
}

pub(super) fn definition() -> CeremonyDefinition {
    let step = CeremonyStep::new(
        step_id(),
        StateId::new("COLLECTING_VOICES").unwrap(),
        StepHandlerKind::new("multiagent_round").unwrap(),
        StepHandlerConfig::empty(),
        RetryPolicy::new(StepAttempt::new(2).unwrap(), DurationMs::ZERO),
        None,
    );
    let guard = CeremonyGuard::new(
        GuardName::new("roundtable_completed").unwrap(),
        GuardCondition::StepStatus {
            step_id: step.id().clone(),
            status: StepStatus::Completed,
        },
    );
    let transition = CeremonyTransition::new(
        StateId::new("COLLECTING_VOICES").unwrap(),
        StateId::new("COMPLETED").unwrap(),
        trigger(),
        vec![guard.name().clone()],
    )
    .unwrap();
    let role = CeremonyRole::new(
        role_id(),
        vec![
            RoleAction::step(step.id().clone()),
            RoleAction::transition(transition.trigger().clone()),
            RoleAction::request_intervention(),
        ],
    )
    .unwrap();
    let respondent = CeremonyRole::new(
        respondent_role_id(),
        vec![RoleAction::respond_to_intervention()],
    )
    .unwrap();

    CeremonyDefinition::new(
        definition_name(),
        version(),
        None,
        Vec::new(),
        Vec::new(),
        vec![
            CeremonyState::initial(StateId::new("COLLECTING_VOICES").unwrap()),
            CeremonyState::terminal(StateId::new("COMPLETED").unwrap()),
        ],
        vec![transition],
        vec![step],
        vec![guard],
        vec![role, respondent],
    )
    .unwrap()
}

/// A two-step linear ceremony (`open` in OPENING, `respond` in
/// RESPONDING) used to prove that step outputs thread forward into the
/// transcript the next step receives.
pub(super) fn two_step_definition() -> CeremonyDefinition {
    let open = CeremonyStep::new(
        StepId::new("open").unwrap(),
        StateId::new("OPENING").unwrap(),
        StepHandlerKind::new("multiagent_round").unwrap(),
        StepHandlerConfig::empty(),
        RetryPolicy::single_attempt(),
        None,
    );
    let respond = CeremonyStep::new(
        StepId::new("respond").unwrap(),
        StateId::new("RESPONDING").unwrap(),
        StepHandlerKind::new("multiagent_round").unwrap(),
        StepHandlerConfig::empty(),
        RetryPolicy::single_attempt(),
        None,
    );
    let open_done = CeremonyGuard::new(
        GuardName::new("open_done").unwrap(),
        GuardCondition::StepStatus {
            step_id: open.id().clone(),
            status: StepStatus::Completed,
        },
    );
    let respond_done = CeremonyGuard::new(
        GuardName::new("respond_done").unwrap(),
        GuardCondition::StepStatus {
            step_id: respond.id().clone(),
            status: StepStatus::Completed,
        },
    );
    let opened = CeremonyTransition::new(
        StateId::new("OPENING").unwrap(),
        StateId::new("RESPONDING").unwrap(),
        TransitionTrigger::new("opened").unwrap(),
        vec![open_done.name().clone()],
    )
    .unwrap();
    let responded = CeremonyTransition::new(
        StateId::new("RESPONDING").unwrap(),
        StateId::new("CLOSED").unwrap(),
        TransitionTrigger::new("responded").unwrap(),
        vec![respond_done.name().clone()],
    )
    .unwrap();
    let role = CeremonyRole::new(
        role_id(),
        vec![
            RoleAction::step(open.id().clone()),
            RoleAction::step(respond.id().clone()),
            RoleAction::transition(opened.trigger().clone()),
            RoleAction::transition(responded.trigger().clone()),
        ],
    )
    .unwrap();

    CeremonyDefinition::new(
        CeremonyName::new("two_step_meeting").unwrap(),
        version(),
        None,
        Vec::new(),
        Vec::new(),
        vec![
            CeremonyState::initial(StateId::new("OPENING").unwrap()),
            CeremonyState::intermediate(StateId::new("RESPONDING").unwrap()),
            CeremonyState::terminal(StateId::new("CLOSED").unwrap()),
        ],
        vec![opened, responded],
        vec![open, respond],
        vec![open_done, respond_done],
        vec![role],
    )
    .unwrap()
}

pub(super) fn approval_definition() -> CeremonyDefinition {
    let guard_name = GuardName::new("human_approved").unwrap();
    let guard = CeremonyGuard::new(guard_name.clone(), GuardCondition::HumanApproval);
    let transition = CeremonyTransition::new(
        StateId::new("STARTED").unwrap(),
        StateId::new("APPROVED").unwrap(),
        TransitionTrigger::new("approve").unwrap(),
        vec![guard_name],
    )
    .unwrap();
    let role = CeremonyRole::new(
        role_id(),
        vec![RoleAction::transition(transition.trigger().clone())],
    )
    .unwrap();

    CeremonyDefinition::new(
        approval_definition_name(),
        version(),
        None,
        Vec::new(),
        Vec::new(),
        vec![
            CeremonyState::initial(StateId::new("STARTED").unwrap()),
            CeremonyState::terminal(StateId::new("APPROVED").unwrap()),
        ],
        vec![transition],
        Vec::new(),
        vec![guard],
        vec![role],
    )
    .unwrap()
}

pub(super) fn started_instance(definition: &CeremonyDefinition) -> CeremonyInstance {
    CeremonyInstance::start(ceremony_id(), definition, CeremonyContext::empty(), now())
}

/// The published catalogue. Empty by default, because most tests run
/// an unbound session; seed it when the point of the test is a session
/// that is bound to what it runs.
#[derive(Debug, Default)]
pub(super) struct PublicationsFake {
    published: RwLock<BTreeMap<(String, String), PublishedCeremonyDefinition>>,
}

impl PublicationsFake {
    pub(super) async fn seed(&self, definition: CeremonyDefinition) -> PublishedCeremonyDefinition {
        let sealed = PublishedCeremonyDefinition::seal(definition).unwrap();
        self.published.write().await.insert(
            (
                sealed.name().as_str().to_owned(),
                sealed.version().as_str().to_owned(),
            ),
            sealed.clone(),
        );
        sealed
    }
}

#[async_trait]
impl CeremonyDefinitionPublicationPort for PublicationsFake {
    async fn publish(
        &self,
        definition: PublishedCeremonyDefinition,
    ) -> Result<PublicationOutcome, DomainError> {
        let key = (
            definition.name().as_str().to_owned(),
            definition.version().as_str().to_owned(),
        );
        let mut published = self.published.write().await;
        if let Some(occupant) = published.get(&key) {
            return Ok(PublicationOutcome::VersionOccupied {
                published: occupant.digest(),
                offered: definition.digest(),
            });
        }
        published.insert(key, definition.clone());
        Ok(PublicationOutcome::Published(definition))
    }

    async fn published(
        &self,
        name: &CeremonyName,
        version: &CeremonyVersion,
    ) -> Result<Option<PublishedCeremonyDefinition>, DomainError> {
        Ok(self
            .published
            .read()
            .await
            .get(&(name.as_str().to_owned(), version.as_str().to_owned()))
            .cloned())
    }

    async fn catalogue(&self) -> Result<Vec<PublishedCeremonyDefinition>, DomainError> {
        Ok(self.published.read().await.values().cloned().collect())
    }
}

/// The resolver every use case that advances a session now takes. Most
/// tests want the plain repository behind it and nothing published.
pub(super) fn definition_resolver(
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
) -> Arc<ResolveCeremonyDefinitionUseCase> {
    resolver_with(definitions, Arc::new(PublicationsFake::default()))
}

pub(super) fn resolver_with(
    definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    publications: Arc<dyn CeremonyDefinitionPublicationPort>,
) -> Arc<ResolveCeremonyDefinitionUseCase> {
    Arc::new(ResolveCeremonyDefinitionUseCase::new(
        definitions,
        publications,
    ))
}

/// Memory that keeps what it was handed, so a test can see what the
/// engine chose to remember — and why it said one thing led to
/// another.
///
/// Not a stand-in for a kernel. What is worth checking here is the
/// engine's judgement, and that is the same whatever backend receives
/// it.
#[derive(Debug, Default)]
pub(super) struct RecordingMemory {
    written: RwLock<Vec<(MemoryScope, MemoryWrite, String)>>,
}

impl RecordingMemory {
    pub(super) async fn entries(&self) -> Vec<MemoryEntry> {
        self.written
            .read()
            .await
            .iter()
            .flat_map(|(_, write, _)| write.entries().to_vec())
            .collect()
    }

    pub(super) async fn relations(&self) -> Vec<MemoryRelation> {
        self.written
            .read()
            .await
            .iter()
            .flat_map(|(_, write, _)| write.relations().to_vec())
            .collect()
    }
}

#[async_trait]
impl MemoryWriterPort for RecordingMemory {
    async fn remember(
        &self,
        scope: &MemoryScope,
        write: MemoryWrite,
        idempotency_key: &str,
    ) -> Result<MemoryWriteOutcome, DomainError> {
        self.written
            .write()
            .await
            .push((scope.clone(), write, idempotency_key.to_owned()));
        Ok(MemoryWriteOutcome::Remembered)
    }

    fn capabilities(&self) -> MemoryCapabilities {
        MemoryCapabilities::none()
            .with(MemoryCapability::Remembering)
            .with(MemoryCapability::KeepingReasons)
    }
}

pub(super) fn recording_memory() -> Arc<RecordingMemory> {
    Arc::new(RecordingMemory::default())
}

pub(super) fn recorder(memory: Arc<RecordingMemory>) -> Arc<SessionMemoryRecorder> {
    Arc::new(SessionMemoryRecorder::new(memory))
}

/// The recorder for the many tests that do not care about memory.
pub(super) fn a_recorder() -> Arc<SessionMemoryRecorder> {
    recorder(recording_memory())
}
