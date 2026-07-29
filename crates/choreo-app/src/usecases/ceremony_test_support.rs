use std::collections::BTreeMap;

use async_trait::async_trait;
use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort,
    CeremonyStepHandlerRequest, CeremonyTranscriptStorePort, ClockPort,
};
use choreo_core::value_objects::{
    CeremonyContext, CeremonyGuard, CeremonyId, CeremonyName, CeremonyRole, CeremonyState,
    CeremonyStep, CeremonyStepContribution, CeremonyTranscript, CeremonyTransition,
    CeremonyVersion, DurationMs, GuardCondition, GuardName, IdempotencyKey, LeaseOwnerId,
    RetryPolicy, RoleAction, RoleId, StateId, StepAttempt, StepHandlerConfig, StepHandlerKind,
    StepId, StepResult, StepStatus, TransitionTrigger,
};
use time::macros::datetime;
use time::OffsetDateTime;
use tokio::sync::RwLock;

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
