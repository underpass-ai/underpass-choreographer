//! Regression test: the use-case layer emits the
//! `#[tracing::instrument]` spans it advertises, with the field
//! names operators wire dashboards against.
//!
//! Kept in its own integration-test binary so the capture subscriber
//! owns the process-global dispatcher without fighting other unit
//! tests over thread-local state.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use choreo_app::usecases::DeliberateUseCase;
use choreo_core::entities::{Council, Deliberation, TaskConstraints, ValidatorReport};
use choreo_core::error::DomainError;
use choreo_core::events::{
    DeliberationCompletedEvent, PhaseChangedEvent, TaskCompletedEvent, TaskDispatchedEvent,
    TaskFailedEvent,
};
use choreo_core::ports::{
    AgentPort, AgentResolverPort, ClockPort, CouncilRegistryPort, Critique,
    DeliberationRepositoryPort, DraftRequest, MessagingPort, Revision, ScoringPort, StatisticsPort,
    ValidatorPort,
};
use choreo_core::value_objects::{
    AgentId, Attributes, CouncilId, DurationMs, Rounds, Rubric, Score, Specialty, TaskDescription,
    TaskId,
};
use time::macros::datetime;
use time::OffsetDateTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

// --- Capture layer ---------------------------------------------------------

#[derive(Default, Clone)]
struct CapturedSpan {
    name: String,
    fields: BTreeMap<String, String>,
}

#[derive(Default, Clone)]
struct CaptureLayer {
    spans: Arc<Mutex<Vec<CapturedSpan>>>,
}

#[derive(Default)]
struct FieldRecorder(BTreeMap<String, String>);
impl tracing::field::Visit for FieldRecorder {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut recorder = FieldRecorder::default();
        attrs.record(&mut recorder);
        self.spans.lock().unwrap().push(CapturedSpan {
            name: attrs.metadata().name().to_owned(),
            fields: recorder.0,
        });
    }
}

fn spans_handle() -> Arc<Mutex<Vec<CapturedSpan>>> {
    static SPANS: OnceLock<Arc<Mutex<Vec<CapturedSpan>>>> = OnceLock::new();
    SPANS
        .get_or_init(|| {
            let spans = Arc::new(Mutex::new(Vec::new()));
            let layer = CaptureLayer {
                spans: spans.clone(),
            };
            tracing_subscriber::registry().with(layer).init();
            spans
        })
        .clone()
}

// --- Fixture ---------------------------------------------------------------

struct FrozenClock;
impl ClockPort for FrozenClock {
    fn now(&self) -> OffsetDateTime {
        datetime!(2026-04-15 12:00:00 UTC)
    }
}

#[derive(Debug)]
struct StubAgent {
    id: AgentId,
    specialty: Specialty,
}
#[async_trait]
impl AgentPort for StubAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }
    fn specialty(&self) -> &Specialty {
        &self.specialty
    }
    async fn generate(&self, _: DraftRequest) -> Result<Revision, DomainError> {
        Ok(Revision {
            content: format!("content-{}", self.id.as_str()),
        })
    }
    async fn critique(&self, _: &str, _: &TaskConstraints) -> Result<Critique, DomainError> {
        Ok(Critique {
            feedback: String::new(),
        })
    }
    async fn revise(&self, own: &str, _: &Critique) -> Result<Revision, DomainError> {
        Ok(Revision {
            content: own.to_owned(),
        })
    }
}

struct StubResolver {
    agents: BTreeMap<AgentId, Arc<dyn AgentPort>>,
}
#[async_trait]
impl AgentResolverPort for StubResolver {
    async fn resolve(&self, id: &AgentId) -> Result<Arc<dyn AgentPort>, DomainError> {
        self.agents
            .get(id)
            .cloned()
            .ok_or(DomainError::NotFound { what: "agent" })
    }
}

struct FixedRegistry {
    council: Council,
}
#[async_trait]
impl CouncilRegistryPort for FixedRegistry {
    async fn register(&self, _: Council) -> Result<(), DomainError> {
        Ok(())
    }
    async fn replace(&self, _: Council) -> Result<(), DomainError> {
        Ok(())
    }
    async fn get(&self, specialty: &Specialty) -> Result<Council, DomainError> {
        if specialty == self.council.specialty() {
            Ok(self.council.clone())
        } else {
            Err(DomainError::NotFound { what: "council" })
        }
    }
    async fn list(&self) -> Result<Vec<Council>, DomainError> {
        Ok(vec![self.council.clone()])
    }
    async fn delete(&self, _: &Specialty) -> Result<(), DomainError> {
        Ok(())
    }
    async fn contains(&self, _: &Specialty) -> Result<bool, DomainError> {
        Ok(true)
    }
}

#[derive(Default)]
struct NullRepo;
#[async_trait]
impl DeliberationRepositoryPort for NullRepo {
    async fn save(&self, _: &Deliberation) -> Result<(), DomainError> {
        Ok(())
    }
    async fn get(&self, _: &TaskId) -> Result<Deliberation, DomainError> {
        Err(DomainError::NotFound {
            what: "deliberation",
        })
    }
    async fn exists(&self, _: &TaskId) -> Result<bool, DomainError> {
        Ok(false)
    }
}

#[derive(Default)]
struct NullBus;
#[async_trait]
impl MessagingPort for NullBus {
    async fn publish_task_dispatched(&self, _: &TaskDispatchedEvent) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_task_completed(&self, _: &TaskCompletedEvent) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_task_failed(&self, _: &TaskFailedEvent) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_deliberation_completed(
        &self,
        _: &DeliberationCompletedEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_phase_changed(&self, _: &PhaseChangedEvent) -> Result<(), DomainError> {
        Ok(())
    }
}

struct PassValidator;
#[async_trait]
impl ValidatorPort for PassValidator {
    fn kind(&self) -> &'static str {
        "pass"
    }
    async fn validate(&self, _: &str, _: &TaskConstraints) -> Result<ValidatorReport, DomainError> {
        ValidatorReport::new("pass", true, "ok", Attributes::empty())
    }
}

struct FullScoring;
#[async_trait]
impl ScoringPort for FullScoring {
    async fn score(&self, _: &[ValidatorReport]) -> Result<Score, DomainError> {
        Score::new(1.0)
    }
}

#[derive(Default)]
struct NullStats;
#[async_trait]
impl StatisticsPort for NullStats {
    async fn record_deliberation(&self, _: &Specialty, _: DurationMs) -> Result<(), DomainError> {
        Ok(())
    }
    async fn record_orchestration(&self, _: DurationMs) -> Result<(), DomainError> {
        Ok(())
    }
    async fn snapshot(&self) -> Result<choreo_core::entities::Statistics, DomainError> {
        Ok(choreo_core::entities::Statistics::default())
    }
}

fn build_usecase() -> DeliberateUseCase {
    let sp = Specialty::new("reviewer").unwrap();
    let a1 = AgentId::new("a1").unwrap();
    let agent: Arc<dyn AgentPort> = Arc::new(StubAgent {
        id: a1.clone(),
        specialty: sp.clone(),
    });
    let mut agents_map: BTreeMap<AgentId, Arc<dyn AgentPort>> = BTreeMap::new();
    agents_map.insert(a1.clone(), agent);

    let council = Council::new(
        CouncilId::new("c").unwrap(),
        sp.clone(),
        vec![a1],
        datetime!(2026-04-15 12:00:00 UTC),
    )
    .unwrap();

    DeliberateUseCase::new(
        Arc::new(FrozenClock),
        Arc::new(FixedRegistry { council }),
        Arc::new(StubResolver { agents: agents_map }),
        vec![Arc::new(PassValidator)],
        Arc::new(FullScoring),
        Arc::new(NullRepo),
        Arc::new(NullBus),
        Arc::new(NullStats),
        "test",
    )
}

fn task() -> choreo_core::entities::Task {
    choreo_core::entities::Task::new(
        TaskId::new("t-span").unwrap(),
        Specialty::new("reviewer").unwrap(),
        TaskDescription::new("x").unwrap(),
        TaskConstraints::new(Rubric::empty(), Rounds::new(0).unwrap(), None, None),
        Attributes::empty(),
    )
}

// --- The test -------------------------------------------------------------

#[tokio::test]
async fn deliberate_use_case_emits_instrumented_span_with_task_fields() {
    // The operator-facing contract: a `deliberate` span MUST be
    // emitted with the fields `task_id`, `specialty`, and `rounds`.
    // Dashboards and tracing queries pin those names; a rename here
    // would break production observability silently.
    let spans = spans_handle();
    spans.lock().unwrap().clear();

    let usecase = build_usecase();
    usecase.execute(task()).await.unwrap();

    let captured = spans.lock().unwrap().clone();
    let names: Vec<&str> = captured.iter().map(|s| s.name.as_str()).collect();
    let deliberate = captured
        .iter()
        .find(|s| s.name == "deliberate")
        .unwrap_or_else(|| panic!("deliberate span must be recorded; saw: {names:?}"));
    assert_eq!(
        deliberate.fields.get("task_id").map(String::as_str),
        Some("t-span")
    );
    assert_eq!(
        deliberate.fields.get("specialty").map(String::as_str),
        Some("reviewer")
    );
    assert_eq!(
        deliberate.fields.get("rounds").map(String::as_str),
        Some("0")
    );
}
