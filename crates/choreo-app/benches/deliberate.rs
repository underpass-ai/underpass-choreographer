//! End-to-end bench for [`DeliberateUseCase`].
//!
//! Exercises the full deliberation loop (seed → peer-review
//! rounds → validate → score → rank → save → publish → record
//! statistics) against a fixture that replaces every port with an
//! in-process stub. The measurement isolates the use case's own
//! cost — no DB, no NATS, no gRPC — so regressions stick out from
//! adapter overhead.
//!
//! Two canonical scenarios:
//! - `deliberate/3-agents-0-rounds`: baseline happy path with no
//!   revision loop. Closest to a minimal invocation.
//! - `deliberate/3-agents-2-rounds`: the aggregate's revision
//!   loop runs twice — stresses the O(rounds · agents) peer review
//!   pairing.

use std::collections::BTreeMap;
use std::sync::Arc;

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
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use time::macros::datetime;
use time::OffsetDateTime;

// ---- Minimal stub ports -----------------------------------------------------

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
            content: "bench-content".to_owned(),
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

// ---- Fixture ----------------------------------------------------------------

fn build_usecase(n_agents: usize) -> DeliberateUseCase {
    let sp = Specialty::new("bench").unwrap();

    let mut agents_map: BTreeMap<AgentId, Arc<dyn AgentPort>> = BTreeMap::new();
    let mut agent_ids = Vec::new();
    for i in 0..n_agents {
        let id = AgentId::new(format!("a{i}")).unwrap();
        agents_map.insert(
            id.clone(),
            Arc::new(StubAgent {
                id: id.clone(),
                specialty: sp.clone(),
            }),
        );
        agent_ids.push(id);
    }

    let council = Council::new(
        CouncilId::new("c").unwrap(),
        sp.clone(),
        agent_ids,
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
        "bench",
    )
}

fn task(rounds: u32) -> choreo_core::entities::Task {
    choreo_core::entities::Task::new(
        TaskId::new("bench-task").unwrap(),
        Specialty::new("bench").unwrap(),
        TaskDescription::new("bench").unwrap(),
        TaskConstraints::new(Rubric::empty(), Rounds::new(rounds).unwrap(), None, None),
        Attributes::empty(),
    )
}

// ---- Benchmarks -------------------------------------------------------------

fn deliberate(c: &mut Criterion) {
    // Tokio runtime shared across iterations — keeps the per-iter
    // cost focused on the use case itself rather than runtime spin-
    // up. `basic_scheduler` (current-thread) mirrors how the real
    // service runs tasks and avoids work-stealing overhead.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("deliberate");
    for (n_agents, rounds) in [(3usize, 0u32), (3, 2)] {
        let usecase = build_usecase(n_agents);
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{n_agents}-agents-{rounds}-rounds")),
            &rounds,
            |b, &rounds| {
                b.to_async(&runtime)
                    .iter(|| async { usecase.execute(task(rounds)).await.unwrap() });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, deliberate);
criterion_main!(benches);
