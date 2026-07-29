//! [`DeliberateUseCase`] — the peer deliberation algorithm.
//!
//! Neutral port of the Python `peer_deliberation_usecase.py`:
//!
//! ```text
//!   1. Proposing      — each agent in the council drafts a proposal
//!   2. Revising       — for `rounds` rounds, every agent critiques a
//!                       peer proposal and the peer's proposal is
//!                       replaced by the revised content
//!   3. Validating     — every proposal is run through the configured
//!                       validators
//!   4. Scoring        — validator reports are aggregated into a Score
//!   5. Completed      — proposals are ranked and the aggregate is
//!                       sealed; a DeliberationCompletedEvent is
//!                       published
//! ```
//!
//! The use case is fully domain-agnostic and provider-agnostic: it
//! depends only on traits from [`choreo_core::ports`]. No vLLM,
//! Anthropic, OpenAI, SWE roles, or Kubernetes lint assumptions leak
//! in here.

use std::sync::Arc;

use choreo_core::entities::{
    Council, Deliberation, Proposal, Task, TaskConstraints, TaskMetadata, ValidationOutcome,
    ValidatorReport,
};
use choreo_core::error::DomainError;
use choreo_core::events::{DeliberationCompletedEvent, EventEnvelope};
use choreo_core::ports::{
    AgentPort, AgentResolverPort, ClockPort, CouncilRegistryPort, DeliberationObserverPort,
    DeliberationRepositoryPort, DraftRequest, MessagingPort, MetricsRecorderPort, NullObserver,
    ScoringPort, StatisticsPort, ValidatorPort,
};
use choreo_core::value_objects::{
    AgentId, DeliberationOutcome, Discrimination, EventId, ProposalId,
};
use time::OffsetDateTime;
use tracing::{debug, info};
use uuid::Uuid;

use super::winner_selection;

/// Result returned by [`DeliberateUseCase::execute`].
#[derive(Debug, Clone)]
pub struct DeliberateOutput {
    pub deliberation: Deliberation,
    pub winner_proposal_id: ProposalId,
}

/// The peer deliberation use case.
pub struct DeliberateUseCase {
    clock: Arc<dyn ClockPort>,
    council_registry: Arc<dyn CouncilRegistryPort>,
    agent_resolver: Arc<dyn AgentResolverPort>,
    validators: Vec<Arc<dyn ValidatorPort>>,
    scoring: Arc<dyn ScoringPort>,
    repository: Arc<dyn DeliberationRepositoryPort>,
    messaging: Arc<dyn MessagingPort>,
    statistics: Arc<dyn StatisticsPort>,
    metrics: Arc<dyn MetricsRecorderPort>,
    source: String,
}

impl std::fmt::Debug for DeliberateUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeliberateUseCase")
            .field("validators", &self.validators.len())
            .field("source", &self.source)
            .finish()
    }
}

impl DeliberateUseCase {
    #[must_use]
    pub fn new(
        clock: Arc<dyn ClockPort>,
        council_registry: Arc<dyn CouncilRegistryPort>,
        agent_resolver: Arc<dyn AgentResolverPort>,
        validators: Vec<Arc<dyn ValidatorPort>>,
        scoring: Arc<dyn ScoringPort>,
        repository: Arc<dyn DeliberationRepositoryPort>,
        messaging: Arc<dyn MessagingPort>,
        statistics: Arc<dyn StatisticsPort>,
        metrics: Arc<dyn MetricsRecorderPort>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            clock,
            council_registry,
            agent_resolver,
            validators,
            scoring,
            repository,
            messaging,
            statistics,
            metrics,
            source: source.into(),
        }
    }

    pub async fn execute(&self, task: Task) -> Result<DeliberateOutput, DomainError> {
        self.execute_with_observer(task, Arc::new(NullObserver))
            .await
    }

    /// Run the deliberation and forward phase transitions to
    /// `observer`. The observer is call-scoped: nothing is retained
    /// by the use case and the observer sees exactly one sequence of
    /// phase transitions per invocation.
    #[tracing::instrument(
        name = "deliberate",
        skip_all,
        fields(
            task_id = %task.id(),
            specialty = %task.specialty(),
            rounds = task.constraints().rounds().get(),
        )
    )]
    pub async fn execute_with_observer(
        &self,
        task: Task,
        observer: Arc<dyn DeliberationObserverPort>,
    ) -> Result<DeliberateOutput, DomainError> {
        let started_at = self.clock.now();

        let council = self.council_registry.get(task.specialty()).await?;
        let agents = self.resolve_agents(&council, task.constraints()).await?;
        if agents.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "deliberation.agents",
            });
        }

        let mut deliberation = Deliberation::start(
            task.id().clone(),
            task.specialty().clone(),
            task.constraints().rounds(),
            started_at,
        );
        observer
            .on_phase_changed(deliberation.task_id(), deliberation.phase(), started_at)
            .await;

        let seeded_proposal_ids = self
            .seed_proposals(&mut deliberation, &agents, &task)
            .await?;
        self.advance_with_observer(&mut deliberation, observer.as_ref())
            .await?; // Proposing -> Revising
        self.run_peer_review_rounds(
            &mut deliberation,
            &agents,
            &seeded_proposal_ids,
            task.constraints(),
        )
        .await?;
        self.advance_with_observer(&mut deliberation, observer.as_ref())
            .await?; // Revising -> Validating
        self.attach_validations(&mut deliberation, task.constraints())
            .await?;
        self.advance_with_observer(&mut deliberation, observer.as_ref())
            .await?; // Validating -> Scoring

        let completed_at = self.clock.now();
        let mut ranked = deliberation.complete(completed_at)?;
        // Whether the scoring policy re-ranked the winner — computed on the
        // pure score order, before the contract-driven valid-first reorder.
        // The policy owns the judge contract, so this stays judge-agnostic.
        let discrimination = self.scoring.discrimination(&ranked);
        observer
            .on_phase_changed(deliberation.task_id(), deliberation.phase(), completed_at)
            .await;
        if task.constraints().output_contract().is_some() {
            ranked = winner_selection::prioritize_valid_outputs(&mut deliberation, &ranked)?;
        }

        self.finalize(deliberation, ranked, &task, completed_at, discrimination)
            .await
    }

    /// Persist and emit a completed deliberation: record statistics and
    /// metrics, select the winner (recording the terminal outcome),
    /// publish the completion event, and return the output. Split out from
    /// the pipeline so `execute_with_observer` reads as the algorithm and
    /// this reads as its close-out effects.
    async fn finalize(
        &self,
        deliberation: Deliberation,
        ranked: Vec<choreo_core::entities::RankedOutcome>,
        task: &Task,
        completed_at: OffsetDateTime,
        discrimination: Option<Discrimination>,
    ) -> Result<DeliberateOutput, DomainError> {
        self.repository.save(&deliberation).await?;

        let duration = deliberation.duration().unwrap_or_default();

        self.statistics
            .record_deliberation(deliberation.specialty(), duration)
            .await?;
        // The duration is recorded for every completed run, before the
        // winner is picked, so a `NoValidProposal` is still timed.
        self.metrics
            .observe_deliberation_duration(deliberation.specialty(), duration);
        if let Some(result) = discrimination {
            self.metrics
                .record_discrimination(deliberation.specialty(), result);
        }

        let winner = match winner_selection::pick_winner(&ranked, task.constraints()) {
            Ok(winner) => winner,
            Err(err) => {
                self.metrics.record_deliberation_outcome(
                    deliberation.specialty(),
                    DeliberationOutcome::NoValidProposal,
                );
                return Err(err);
            }
        };
        self.metrics
            .record_deliberation_outcome(deliberation.specialty(), DeliberationOutcome::Success);
        self.metrics
            .observe_winner_score(deliberation.specialty(), winner.outcome().score());

        let completion_event = DeliberationCompletedEvent::new_with_context(
            self.envelope(completed_at, task.metadata())?,
            deliberation.task_id().clone(),
            deliberation.specialty().clone(),
            winner.proposal().id().clone(),
            winner.outcome().score(),
            u32::try_from(ranked.len()).unwrap_or(u32::MAX),
            duration,
            task.external_context()
                .map(|bundle| bundle.bundle_id().to_owned()),
        );
        self.messaging
            .publish_deliberation_completed(&completion_event)
            .await?;

        info!(
            task_id = deliberation.task_id().as_str(),
            specialty = deliberation.specialty().as_str(),
            winner_id = winner.proposal().id().as_str(),
            winner_score = winner.outcome().score().get(),
            duration_ms = duration.get(),
            "deliberation completed"
        );

        Ok(DeliberateOutput {
            winner_proposal_id: winner.proposal().id().clone(),
            deliberation,
        })
    }

    async fn advance_with_observer(
        &self,
        deliberation: &mut Deliberation,
        observer: &dyn DeliberationObserverPort,
    ) -> Result<(), DomainError> {
        deliberation.advance()?;
        observer
            .on_phase_changed(
                deliberation.task_id(),
                deliberation.phase(),
                self.clock.now(),
            )
            .await;
        Ok(())
    }

    async fn resolve_agents(
        &self,
        council: &Council,
        constraints: &TaskConstraints,
    ) -> Result<Vec<Arc<dyn AgentPort>>, DomainError> {
        let mut ids: Vec<AgentId> = council.agents().iter().cloned().collect();
        if let Some(n) = constraints.num_agents() {
            let cap = n.get() as usize;
            if ids.len() > cap {
                ids.truncate(cap);
            }
        }
        self.agent_resolver.resolve_all(&ids).await
    }

    /// Seed one proposal per agent and return the list of proposal
    /// ids **in the same order as `agents`**. This preserves the
    /// agent→proposal mapping which the peer-review round algorithm
    /// relies on to pick the correct peer (e.g. agent `i` critiques
    /// the proposal authored by agent `(i+1) % N`).
    ///
    /// Using `deliberation.proposals().keys()` instead would iterate
    /// in sorted `ProposalId` order (the UUID lexicographic order)
    /// which has no relation to agent order and would scramble the
    /// peer-review pairing.
    async fn seed_proposals(
        &self,
        deliberation: &mut Deliberation,
        agents: &[Arc<dyn AgentPort>],
        task: &Task,
    ) -> Result<Vec<ProposalId>, DomainError> {
        let now = self.clock.now();
        let mut ordered_ids = Vec::with_capacity(agents.len());
        for agent in agents {
            let draft = agent
                .generate(DraftRequest {
                    task: task.description().clone(),
                    constraints: task.constraints().clone(),
                    diverse: true,
                    external_context: task.external_context().cloned(),
                })
                .await?;
            let proposal_id = new_proposal_id()?;
            let content_len = draft.content.len();
            let preview = content_preview(&draft.content);
            let proposal = Proposal::new(
                proposal_id.clone(),
                agent.id().clone(),
                task.specialty().clone(),
                draft.content,
                task.attributes().clone(),
                now,
            )?;
            deliberation.add_proposal(proposal)?;
            // Span event: each agent's opening proposal, observable in the
            // OTEL trace as the "what each participant said" of the meeting.
            info!(
                proposal_id = proposal_id.as_str(),
                author = agent.id().as_str(),
                content_len,
                preview = %preview,
                "proposal drafted"
            );
            ordered_ids.push(proposal_id);
        }
        Ok(ordered_ids)
    }

    async fn run_peer_review_rounds(
        &self,
        deliberation: &mut Deliberation,
        agents: &[Arc<dyn AgentPort>],
        ordered_proposal_ids: &[ProposalId],
        constraints: &TaskConstraints,
    ) -> Result<(), DomainError> {
        let rounds = constraints.rounds().get();
        if rounds == 0 || agents.len() < 2 {
            return Ok(());
        }
        if ordered_proposal_ids.len() != agents.len() {
            // Invariant: one proposal per agent after seeding. If this
            // fires it is a bug in the seeding step, not adapter-shaped.
            return Err(DomainError::InvariantViolated {
                reason: "proposal count must match agent count before peer review",
            });
        }

        for round in 0..rounds {
            for (i, agent) in agents.iter().enumerate() {
                let peer_idx = (i + 1) % agents.len();
                let peer_proposal_id = ordered_proposal_ids[peer_idx].clone();
                let peer_content = deliberation
                    .proposals()
                    .get(&peer_proposal_id)
                    .map(|p| p.content().to_owned())
                    .ok_or(DomainError::NotFound {
                        what: "deliberation.proposal",
                    })?;
                let critique = agent.critique(&peer_content, constraints).await?;
                let revision = agent.revise(&peer_content, &critique).await?;
                let now = self.clock.now();
                let revised_preview = content_preview(&revision.content);
                deliberation.revise_proposal(&peer_proposal_id, revision.content, now)?;
                // Span event: the adversarial peer review — agent i critiques
                // and revises agent (i+1)'s proposal. The back-and-forth of the
                // meeting, observable in the trace.
                info!(
                    round = round + 1,
                    reviewer = agent.id().as_str(),
                    target_proposal = peer_proposal_id.as_str(),
                    revised_preview = %revised_preview,
                    "peer critique and revision"
                );
            }
            debug!(round = round + 1, "peer review round completed");
        }
        Ok(())
    }

    async fn attach_validations(
        &self,
        deliberation: &mut Deliberation,
        constraints: &TaskConstraints,
    ) -> Result<(), DomainError> {
        let proposal_ids: Vec<ProposalId> = deliberation.proposals().keys().cloned().collect();
        for id in proposal_ids {
            let content = deliberation
                .proposals()
                .get(&id)
                .map(|p| p.content().to_owned())
                .ok_or(DomainError::NotFound {
                    what: "deliberation.proposal",
                })?;

            let reports = self.run_validators(&content, constraints).await?;
            // Span events: every validator's verdict on this proposal. The LLM
            // judge surfaces here too — its report `summary` carries the score
            // (e.g. "llm judge score 0.87") — so the judging is observable in
            // the trace without coupling this use case to any provider.
            for report in &reports {
                info!(
                    proposal_id = id.as_str(),
                    validator = report.kind(),
                    passed = report.passed(),
                    verdict = report.summary(),
                    "validator verdict"
                );
            }
            let score = self.scoring.score(&reports).await?;
            info!(
                proposal_id = id.as_str(),
                score = score.get(),
                "proposal scored"
            );
            let outcome = ValidationOutcome::new(score, reports);
            deliberation.attach_outcome(&id, outcome)?;
        }
        Ok(())
    }

    async fn run_validators(
        &self,
        content: &str,
        constraints: &TaskConstraints,
    ) -> Result<Vec<ValidatorReport>, DomainError> {
        let mut reports = Vec::with_capacity(self.validators.len());
        for validator in &self.validators {
            let report = validator.validate(content, constraints).await?;
            reports.push(report);
        }
        Ok(reports)
    }

    fn envelope(
        &self,
        emitted_at: OffsetDateTime,
        metadata: &TaskMetadata,
    ) -> Result<EventEnvelope, DomainError> {
        EventEnvelope::new_with_causation(
            new_event_id()?,
            emitted_at,
            self.source.clone(),
            metadata.correlation_id().cloned(),
            metadata.causation_id().cloned(),
        )
    }
}

fn new_proposal_id() -> Result<ProposalId, DomainError> {
    ProposalId::new(Uuid::new_v4().to_string())
}

/// A single-line preview of proposal/revision content for trace events —
/// enough to read each competing proposal in a span without carrying the
/// full payload (the winning text per step lives in full in the
/// `RunCeremony` response and the ceremony transcript).
const PREVIEW_MAX: usize = 700;
fn content_preview(content: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= PREVIEW_MAX {
        return flat;
    }
    let truncated: String = flat.chars().take(PREVIEW_MAX).collect();
    format!("{truncated}…")
}

fn new_event_id() -> Result<EventId, DomainError> {
    EventId::new(Uuid::new_v4().to_string())
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_core::entities::{
        ContextItem, ContextReference, ContextSummary, DeliberationPhase, ExternalContextBundle,
        Task, TaskConstraints,
    };
    use choreo_core::events::{
        DeliberationCompletedEvent, PhaseChangedEvent, TaskCompletedEvent, TaskDispatchedEvent,
        TaskFailedEvent,
    };
    use choreo_core::ports::{Critique, Revision};
    use choreo_core::value_objects::{
        Attributes, CouncilId, NumAgents, OutputContract, OutputFieldRule, Rounds, Rubric, Score,
        Specialty, TaskDescription, TaskId,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::macros::datetime;

    // --- Clock ------------------------------------------------------------
    struct FrozenClock {
        now: OffsetDateTime,
    }
    impl ClockPort for FrozenClock {
        fn now(&self) -> OffsetDateTime {
            self.now
        }
    }

    // --- Agent -----------------------------------------------------------
    #[derive(Debug)]
    struct StubAgent {
        id: AgentId,
        specialty: Specialty,
        draft: String,
        revise_to: Mutex<Vec<String>>, // per-round revised contents
    }
    impl StubAgent {
        fn new(id: &str, specialty: &Specialty, draft: &str, revisions: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                id: AgentId::new(id).unwrap(),
                specialty: specialty.clone(),
                draft: draft.to_owned(),
                revise_to: Mutex::new(revisions.into_iter().map(String::from).collect()),
            })
        }
    }
    #[async_trait]
    impl AgentPort for StubAgent {
        fn id(&self) -> &AgentId {
            &self.id
        }
        fn specialty(&self) -> &Specialty {
            &self.specialty
        }
        async fn generate(&self, _request: DraftRequest) -> Result<Revision, DomainError> {
            Ok(Revision {
                content: self.draft.clone(),
            })
        }
        async fn critique(
            &self,
            _peer_content: &str,
            _constraints: &TaskConstraints,
        ) -> Result<Critique, DomainError> {
            Ok(Critique {
                feedback: format!("feedback from {}", self.id),
            })
        }
        async fn revise(
            &self,
            own_content: &str,
            _critique: &Critique,
        ) -> Result<Revision, DomainError> {
            let mut v = self.revise_to.lock().unwrap();
            if v.is_empty() {
                Ok(Revision {
                    content: format!("{own_content}*"),
                })
            } else {
                Ok(Revision {
                    content: v.remove(0),
                })
            }
        }
    }

    // --- Resolver ---------------------------------------------------------
    struct StubResolver {
        agents: std::collections::BTreeMap<AgentId, Arc<dyn AgentPort>>,
    }
    impl StubResolver {
        fn new(agents: Vec<Arc<dyn AgentPort>>) -> Self {
            let mut m = std::collections::BTreeMap::new();
            for a in agents {
                m.insert(a.id().clone(), a);
            }
            Self { agents: m }
        }
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

    // --- Registry ---------------------------------------------------------
    struct FixedRegistry {
        council: Council,
    }
    #[async_trait]
    impl CouncilRegistryPort for FixedRegistry {
        async fn register(&self, _council: Council) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn replace(&self, _council: Council) -> Result<(), DomainError> {
            unimplemented!()
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
        async fn delete(&self, _specialty: &Specialty) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn contains(&self, specialty: &Specialty) -> Result<bool, DomainError> {
            Ok(specialty == self.council.specialty())
        }
    }

    // --- Repository -------------------------------------------------------
    #[derive(Default)]
    struct InMemoryRepo {
        saved: Mutex<Vec<Deliberation>>,
    }
    #[async_trait]
    impl DeliberationRepositoryPort for InMemoryRepo {
        async fn save(&self, deliberation: &Deliberation) -> Result<(), DomainError> {
            self.saved.lock().unwrap().push(deliberation.clone());
            Ok(())
        }
        async fn get(&self, task_id: &TaskId) -> Result<Deliberation, DomainError> {
            self.saved
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|d| d.task_id() == task_id)
                .cloned()
                .ok_or(DomainError::NotFound {
                    what: "deliberation",
                })
        }
        async fn exists(&self, task_id: &TaskId) -> Result<bool, DomainError> {
            Ok(self
                .saved
                .lock()
                .unwrap()
                .iter()
                .any(|d| d.task_id() == task_id))
        }
    }

    // --- Messaging --------------------------------------------------------
    #[derive(Default)]
    struct NullBus {
        completed: Mutex<Vec<DeliberationCompletedEvent>>,
    }
    #[async_trait]
    impl MessagingPort for NullBus {
        async fn publish_task_dispatched(
            &self,
            _event: &TaskDispatchedEvent,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn publish_task_completed(
            &self,
            _event: &TaskCompletedEvent,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn publish_task_failed(&self, _event: &TaskFailedEvent) -> Result<(), DomainError> {
            Ok(())
        }
        async fn publish_deliberation_completed(
            &self,
            event: &DeliberationCompletedEvent,
        ) -> Result<(), DomainError> {
            self.completed.lock().unwrap().push(event.clone());
            Ok(())
        }
        async fn publish_phase_changed(
            &self,
            _event: &PhaseChangedEvent,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    // --- Validator + Scoring ---------------------------------------------
    struct ContentLengthValidator;
    #[async_trait]
    impl ValidatorPort for ContentLengthValidator {
        fn kind(&self) -> &'static str {
            "content-length"
        }
        async fn validate(
            &self,
            content: &str,
            _constraints: &TaskConstraints,
        ) -> Result<ValidatorReport, DomainError> {
            let passed = content.len() >= 3;
            ValidatorReport::new(
                self.kind(),
                passed,
                format!("len={}", content.len()),
                Attributes::empty(),
            )
        }
    }

    /// A validator whose `validate` always fails with a transport-shaped
    /// error — models e.g. the LLM judge hitting an HTTP/timeout/parse
    /// failure (it returns `Err`, not a failing-but-well-formed report).
    struct FailingValidator;
    #[async_trait]
    impl ValidatorPort for FailingValidator {
        fn kind(&self) -> &'static str {
            "failing"
        }
        async fn validate(
            &self,
            _content: &str,
            _constraints: &TaskConstraints,
        ) -> Result<ValidatorReport, DomainError> {
            Err(DomainError::InvariantViolated {
                reason: "validator boom",
            })
        }
    }

    struct LinearScoring;
    #[async_trait]
    impl ScoringPort for LinearScoring {
        async fn score(&self, reports: &[ValidatorReport]) -> Result<Score, DomainError> {
            if reports.is_empty() {
                return Score::new(0.0);
            }
            let passed = reports.iter().filter(|r| r.passed()).count() as f64;
            let total = reports.len() as f64;
            Score::new(passed / total)
        }
    }

    /// Scores like [`LinearScoring`] but always reports a `Reranked`
    /// discrimination, so a test can assert the use case forwards it.
    struct RerankingScoring;
    #[async_trait]
    impl ScoringPort for RerankingScoring {
        async fn score(&self, reports: &[ValidatorReport]) -> Result<Score, DomainError> {
            if reports.is_empty() {
                return Score::new(0.0);
            }
            let passed = reports.iter().filter(|r| r.passed()).count() as f64;
            Score::new(passed / reports.len() as f64)
        }
        fn discrimination(
            &self,
            _ranked: &[choreo_core::entities::RankedOutcome],
        ) -> Option<choreo_core::value_objects::Discrimination> {
            Some(choreo_core::value_objects::Discrimination::Reranked)
        }
    }

    #[derive(Default)]
    struct NullStats;
    #[async_trait]
    impl choreo_core::ports::StatisticsPort for NullStats {
        async fn record_deliberation(
            &self,
            _specialty: &Specialty,
            _duration: choreo_core::value_objects::DurationMs,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn record_orchestration(
            &self,
            _duration: choreo_core::value_objects::DurationMs,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn snapshot(&self) -> Result<choreo_core::entities::Statistics, DomainError> {
            Ok(choreo_core::entities::Statistics::default())
        }
    }

    // --- Metrics ----------------------------------------------------------

    use choreo_core::ports::{MetricsRecorderPort, NoopMetricsRecorder};
    use choreo_core::value_objects::{DeliberationOutcome, Discrimination, DurationMs};

    /// Captures every observation so tests can assert what the use case
    /// recorded, keyed by specialty.
    #[derive(Default)]
    struct RecordingMetrics {
        durations: Mutex<Vec<(String, u64)>>,
        outcomes: Mutex<Vec<(String, &'static str)>>,
        winner_scores: Mutex<Vec<(String, f64)>>,
        discriminations: Mutex<Vec<(String, &'static str)>>,
    }
    impl MetricsRecorderPort for RecordingMetrics {
        fn observe_deliberation_duration(&self, specialty: &Specialty, duration: DurationMs) {
            self.durations
                .lock()
                .unwrap()
                .push((specialty.as_str().to_owned(), duration.get()));
        }
        fn record_deliberation_outcome(&self, specialty: &Specialty, outcome: DeliberationOutcome) {
            self.outcomes
                .lock()
                .unwrap()
                .push((specialty.as_str().to_owned(), outcome.as_label()));
        }
        fn observe_winner_score(&self, specialty: &Specialty, score: Score) {
            self.winner_scores
                .lock()
                .unwrap()
                .push((specialty.as_str().to_owned(), score.get()));
        }
        fn observe_judge_latency(&self, _model: &str, _duration: DurationMs) {}
        fn observe_judge_score(&self, _model: &str, _score: Score) {}
        fn record_judge_error(
            &self,
            _model: &str,
            _kind: choreo_core::value_objects::LlmErrorKind,
        ) {
        }
        fn record_provider_error(
            &self,
            _provider: &str,
            _kind: choreo_core::value_objects::LlmErrorKind,
        ) {
        }
        fn record_judge_tokens(
            &self,
            _model: &str,
            _usage: choreo_core::value_objects::TokenUsage,
        ) {
        }
        fn record_provider_tokens(
            &self,
            _provider: &str,
            _usage: choreo_core::value_objects::TokenUsage,
        ) {
        }
        fn observe_provider_request(
            &self,
            _provider: &str,
            _operation: &str,
            _duration: DurationMs,
        ) {
        }
        fn inc_provider_in_flight(&self, _provider: &str) {}
        fn dec_provider_in_flight(&self, _provider: &str) {}
        fn record_discrimination(&self, specialty: &Specialty, result: Discrimination) {
            self.discriminations
                .lock()
                .unwrap()
                .push((specialty.as_str().to_owned(), result.as_label()));
        }
        fn record_ceremony_outcome(
            &self,
            _ceremony: &str,
            _outcome: choreo_core::value_objects::CeremonyOutcome,
        ) {
        }
        fn observe_ceremony_duration(&self, _ceremony: &str, _duration: DurationMs) {}
        fn observe_ceremony_step_duration(
            &self,
            _ceremony: &str,
            _step: &str,
            _duration: DurationMs,
        ) {
        }
        fn record_ceremony_step(
            &self,
            _ceremony: &str,
            _step: &str,
            _status: choreo_core::value_objects::StepStatus,
        ) {
        }
        fn record_ceremony_transition_blocked(&self, _ceremony: &str, _from_state: &str) {}
        fn observe_nats_publish(&self, _subject_kind: &str, _duration: DurationMs) {}
        fn record_nats_publish_error(&self, _subject_kind: &str, _reason: &str) {}
        fn set_postgres_pool_in_use(&self, _connections: i64) {}
        fn record_scoring_mode(&self, _mode: choreo_core::value_objects::ScoringMode) {}
    }

    // --- Fixture helpers --------------------------------------------------

    fn specialty() -> Specialty {
        Specialty::new("reviewer").unwrap()
    }

    fn council_with(agent_ids: &[&str]) -> Council {
        let agents = agent_ids
            .iter()
            .map(|id| AgentId::new(*id).unwrap())
            .collect::<Vec<_>>();
        Council::new(
            CouncilId::new("c").unwrap(),
            specialty(),
            agents,
            datetime!(2026-04-15 12:00:00 UTC),
        )
        .unwrap()
    }

    fn task(constraints: TaskConstraints) -> Task {
        Task::new(
            TaskId::new("t1").unwrap(),
            specialty(),
            TaskDescription::new("describe the proposed change").unwrap(),
            constraints,
            Attributes::empty(),
        )
    }

    fn task_with_context(
        constraints: TaskConstraints,
        external_context: ExternalContextBundle,
    ) -> Task {
        Task::new_with_context(
            TaskId::new("t1").unwrap(),
            specialty(),
            TaskDescription::new("describe the proposed change").unwrap(),
            constraints,
            Attributes::empty(),
            Some(external_context),
        )
    }

    fn sample_external_context() -> ExternalContextBundle {
        ExternalContextBundle::new(
            "ctx-1",
            "v1",
            Some(
                ContextSummary::new(
                    "Bounded caller-supplied context",
                    Attributes::new(BTreeMap::from([(
                        "origin".to_owned(),
                        json!("external-system"),
                    )]))
                    .unwrap(),
                )
                .unwrap(),
            ),
            vec![ContextItem::new(
                "item-1",
                "finding",
                "Primary observation",
                Some("An upstream dependency changed shortly before the task".to_owned()),
                Attributes::new(BTreeMap::from([("weight".to_owned(), json!(0.9))])).unwrap(),
                vec!["ref-1".to_owned()],
            )
            .unwrap()],
            vec![ContextReference::new(
                "ref-1",
                "s3://bucket/evidence.json",
                Some("Evidence".to_owned()),
                Some("application/json".to_owned()),
                Attributes::empty(),
            )
            .unwrap()],
            Attributes::empty(),
        )
        .unwrap()
    }

    /// Build a use case with every collaborator supplied — the single
    /// constructor the typed fixtures below delegate to, so the metrics
    /// recorder (and scoring/validators) can vary per test without
    /// duplicating the wiring.
    fn build_usecase(
        agents: Vec<Arc<dyn AgentPort>>,
        council: Council,
        scoring: Arc<dyn ScoringPort>,
        validators: Vec<Arc<dyn ValidatorPort>>,
        metrics: Arc<dyn MetricsRecorderPort>,
    ) -> (DeliberateUseCase, Arc<InMemoryRepo>, Arc<NullBus>) {
        let clock = Arc::new(FrozenClock {
            now: datetime!(2026-04-15 12:00:00 UTC),
        });
        let registry = Arc::new(FixedRegistry { council });
        let resolver = Arc::new(StubResolver::new(agents));
        let repo = Arc::new(InMemoryRepo::default());
        let bus = Arc::new(NullBus::default());

        let stats: Arc<dyn choreo_core::ports::StatisticsPort> = Arc::new(NullStats);
        let usecase = DeliberateUseCase::new(
            clock,
            registry,
            resolver,
            validators,
            scoring,
            repo.clone(),
            bus.clone(),
            stats,
            metrics,
            "choreographer",
        );
        (usecase, repo, bus)
    }

    fn fixture(
        agents: Vec<Arc<dyn AgentPort>>,
        council: Council,
    ) -> (DeliberateUseCase, Arc<InMemoryRepo>, Arc<NullBus>) {
        build_usecase(
            agents,
            council,
            Arc::new(LinearScoring),
            vec![Arc::new(ContentLengthValidator)],
            Arc::new(NoopMetricsRecorder),
        )
    }

    /// Like [`fixture`] but with a caller-supplied metrics recorder so a
    /// test can assert what the deliberation observed.
    fn fixture_with_metrics(
        agents: Vec<Arc<dyn AgentPort>>,
        council: Council,
        metrics: Arc<dyn MetricsRecorderPort>,
    ) -> (DeliberateUseCase, Arc<InMemoryRepo>, Arc<NullBus>) {
        build_usecase(
            agents,
            council,
            Arc::new(LinearScoring),
            vec![Arc::new(ContentLengthValidator)],
            metrics,
        )
    }

    // --- Tests ------------------------------------------------------------

    #[tokio::test]
    async fn happy_path_returns_winner_and_persists_deliberation() {
        let council = council_with(&["a1", "a2", "a3"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("a1", &sp, "short", vec!["short"]) as Arc<dyn AgentPort>,
            StubAgent::new(
                "a2",
                &sp,
                "medium proposal",
                vec!["medium proposal revised"],
            ) as Arc<dyn AgentPort>,
            StubAgent::new(
                "a3",
                &sp,
                "long rich proposal",
                vec!["long rich proposal v2"],
            ) as Arc<dyn AgentPort>,
        ];
        let (usecase, repo, bus) = fixture(agents, council);

        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(1).unwrap(),
            None,
            None,
        ));
        let out = usecase.execute(t).await.unwrap();

        assert_eq!(out.deliberation.phase(), DeliberationPhase::Completed);
        assert_eq!(out.deliberation.proposals().len(), 3);
        assert_eq!(out.deliberation.outcomes().len(), 3);
        assert_eq!(repo.saved.lock().unwrap().len(), 1);
        assert_eq!(bus.completed.lock().unwrap().len(), 1);

        let event = &bus.completed.lock().unwrap()[0];
        assert_eq!(event.task_id().as_str(), "t1");
        assert_eq!(event.specialty().as_str(), "reviewer");
        assert_eq!(event.num_candidates(), 3);
    }

    #[tokio::test]
    async fn zero_rounds_skips_peer_review_phase_loop() {
        let council = council_with(&["a1", "a2"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("a1", &sp, "AAA", vec![]) as Arc<dyn AgentPort>,
            StubAgent::new("a2", &sp, "BBB", vec![]) as Arc<dyn AgentPort>,
        ];
        let (usecase, _repo, _bus) = fixture(agents, council);

        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(0).unwrap(),
            None,
            None,
        ));
        let out = usecase.execute(t).await.unwrap();

        // No revisions happened: revision_count stays 0 on every proposal.
        for proposal in out.deliberation.proposals().values() {
            assert_eq!(proposal.revision_count(), 0);
        }
    }

    #[tokio::test]
    async fn num_agents_cap_truncates_council() {
        let council = council_with(&["a1", "a2", "a3", "a4", "a5"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = (1..=5)
            .map(|i| {
                StubAgent::new(&format!("a{i}"), &sp, &format!("content {i}"), vec![])
                    as Arc<dyn AgentPort>
            })
            .collect();
        let (usecase, _repo, _bus) = fixture(agents, council);

        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(0).unwrap(),
            Some(NumAgents::new(2).unwrap()),
            None,
        ));
        let out = usecase.execute(t).await.unwrap();

        assert_eq!(out.deliberation.proposals().len(), 2);
    }

    #[tokio::test]
    async fn missing_council_is_reported_as_domain_not_found() {
        let council = council_with(&["a1"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> =
            vec![StubAgent::new("a1", &sp, "x", vec![]) as Arc<dyn AgentPort>];
        let (usecase, _repo, _bus) = fixture(agents, council);

        let other = Task::new(
            TaskId::new("t2").unwrap(),
            Specialty::new("unknown").unwrap(),
            TaskDescription::new("x").unwrap(),
            TaskConstraints::default(),
            Attributes::empty(),
        );
        let err = usecase.execute(other).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { what: "council" }));
    }

    #[tokio::test]
    async fn validation_scores_drive_the_ranking() {
        let council = council_with(&["short", "medium", "long"]);
        let sp = specialty();
        // "short" agent produces content below the validator's length
        // threshold; it should rank behind the others despite being
        // first by id.
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("short", &sp, "ok", vec![]) as Arc<dyn AgentPort>,
            StubAgent::new("medium", &sp, "abcdef", vec![]) as Arc<dyn AgentPort>,
            StubAgent::new("long", &sp, "abcdefghij", vec![]) as Arc<dyn AgentPort>,
        ];
        let (usecase, _repo, bus) = fixture(agents, council);

        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(0).unwrap(),
            None,
            None,
        ));
        let out = usecase.execute(t).await.unwrap();

        let event = &bus.completed.lock().unwrap()[0];
        // The winner must not be the "short" proposal whose content is
        // below the validator threshold.
        assert_ne!(
            out.deliberation
                .proposals()
                .get(event.winner_proposal_id())
                .unwrap()
                .author()
                .as_str(),
            "short"
        );
    }

    /// Regression test: peer-review pairs agent `i` with proposal
    /// `(i+1) % N`. An earlier version of `run_peer_review_rounds`
    /// iterated `deliberation.proposals().keys()` (sorted by
    /// ProposalId, i.e. random UUID order) which scrambled the pairing
    /// silently. This test would fail under that buggy ordering.
    ///
    /// Setup: a bespoke agent stub whose `revise` returns
    /// `"<agent_id>_saw:<own_content>"`. After one round of peer
    /// review each proposal's content identifies the reviewer — so we
    /// can check that agent `i` revised the proposal of agent
    /// `(i+1) % N`.
    #[tokio::test]
    async fn peer_review_pairs_each_agent_with_its_neighbour() {
        #[derive(Debug)]
        struct MarkerAgent {
            id: AgentId,
            specialty: Specialty,
            draft: String,
        }
        #[async_trait]
        impl AgentPort for MarkerAgent {
            fn id(&self) -> &AgentId {
                &self.id
            }
            fn specialty(&self) -> &Specialty {
                &self.specialty
            }
            async fn generate(&self, _request: DraftRequest) -> Result<Revision, DomainError> {
                Ok(Revision {
                    content: self.draft.clone(),
                })
            }
            async fn critique(
                &self,
                _peer_content: &str,
                _constraints: &TaskConstraints,
            ) -> Result<Critique, DomainError> {
                Ok(Critique {
                    feedback: String::new(),
                })
            }
            async fn revise(
                &self,
                own_content: &str,
                _critique: &Critique,
            ) -> Result<Revision, DomainError> {
                Ok(Revision {
                    content: format!("{}_saw:{own_content}", self.id),
                })
            }
        }

        let sp = specialty();
        let make = |id: &str, draft: &str| {
            Arc::new(MarkerAgent {
                id: AgentId::new(id).unwrap(),
                specialty: sp.clone(),
                draft: draft.to_owned(),
            }) as Arc<dyn AgentPort>
        };

        // The use case resolves the council's agents in sorted-id
        // order (BTreeSet iteration), so `agents[i]` below is the
        // sorted order: a1, a2, a3.
        let agents = vec![make("a1", "A"), make("a2", "B"), make("a3", "C")];
        let council = council_with(&["a1", "a2", "a3"]);
        let (usecase, _repo, _bus) = fixture(agents, council);

        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(1).unwrap(),
            None,
            None,
        ));
        let out = usecase.execute(t).await.unwrap();

        // Build a map author_id -> content after the round.
        let by_author: std::collections::HashMap<String, String> = out
            .deliberation
            .proposals()
            .values()
            .map(|p| (p.author().as_str().to_owned(), p.content().to_owned()))
            .collect();

        // Expected rotation:
        //   agent a1 (index 0) critiqued proposal of a2 (index 1) → a1_saw:B
        //   agent a2 (index 1) critiqued proposal of a3 (index 2) → a2_saw:C
        //   agent a3 (index 2) critiqued proposal of a1 (index 0) → a3_saw:A
        assert_eq!(
            by_author["a1"], "a3_saw:A",
            "a1's proposal was revised by a3"
        );
        assert_eq!(
            by_author["a2"], "a1_saw:B",
            "a2's proposal was revised by a1"
        );
        assert_eq!(
            by_author["a3"], "a2_saw:C",
            "a3's proposal was revised by a2"
        );

        // Every proposal must have been revised exactly once.
        for p in out.deliberation.proposals().values() {
            assert_eq!(p.revision_count(), 1);
        }
    }

    // ---- Observer tests --------------------------------------------------

    #[derive(Default)]
    struct RecordingObserver {
        phases: Mutex<Vec<DeliberationPhase>>,
    }
    #[async_trait]
    impl choreo_core::ports::DeliberationObserverPort for RecordingObserver {
        async fn on_phase_changed(
            &self,
            _task_id: &TaskId,
            phase: DeliberationPhase,
            _emitted_at: OffsetDateTime,
        ) {
            self.phases.lock().unwrap().push(phase);
        }
    }

    #[tokio::test]
    async fn execute_with_observer_emits_every_phase_transition_in_order() {
        let council = council_with(&["a1", "a2"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("a1", &sp, "abcdef", vec!["abcdef"]) as Arc<dyn AgentPort>,
            StubAgent::new("a2", &sp, "ghijkl", vec!["ghijkl"]) as Arc<dyn AgentPort>,
        ];
        let (usecase, _repo, _bus) = fixture(agents, council);

        let observer = Arc::new(RecordingObserver::default());
        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(1).unwrap(),
            None,
            None,
        ));
        usecase
            .execute_with_observer(t, observer.clone())
            .await
            .unwrap();

        let phases = observer.phases.lock().unwrap().clone();
        assert_eq!(
            phases,
            vec![
                DeliberationPhase::Proposing,
                DeliberationPhase::Revising,
                DeliberationPhase::Validating,
                DeliberationPhase::Scoring,
                DeliberationPhase::Completed,
            ],
            "observer must see every FSM state exactly once and in order"
        );
    }

    #[tokio::test]
    async fn execute_without_observer_still_runs_through_null_observer() {
        // `execute` is a thin wrapper over `execute_with_observer` with
        // a `NullObserver`; the end-to-end invariant (winner emitted +
        // persistence) must hold regardless.
        let council = council_with(&["a1"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> =
            vec![StubAgent::new("a1", &sp, "plenty", vec![]) as Arc<dyn AgentPort>];
        let (usecase, repo, _bus) = fixture(agents, council);

        let t = task(TaskConstraints::new(
            Rubric::empty(),
            Rounds::new(0).unwrap(),
            None,
            None,
        ));
        usecase.execute(t).await.unwrap();
        assert_eq!(repo.saved.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn draft_request_preserves_external_context_bundle() {
        #[derive(Debug)]
        struct CapturingAgent {
            id: AgentId,
            specialty: Specialty,
            seen_requests: Mutex<Vec<DraftRequest>>,
        }

        #[async_trait]
        impl AgentPort for CapturingAgent {
            fn id(&self) -> &AgentId {
                &self.id
            }

            fn specialty(&self) -> &Specialty {
                &self.specialty
            }

            async fn generate(&self, request: DraftRequest) -> Result<Revision, DomainError> {
                self.seen_requests.lock().unwrap().push(request);
                Ok(Revision {
                    content: "captured".to_owned(),
                })
            }

            async fn critique(
                &self,
                _peer_content: &str,
                _constraints: &TaskConstraints,
            ) -> Result<Critique, DomainError> {
                Ok(Critique {
                    feedback: "ok".to_owned(),
                })
            }

            async fn revise(
                &self,
                own_content: &str,
                _critique: &Critique,
            ) -> Result<Revision, DomainError> {
                Ok(Revision {
                    content: own_content.to_owned(),
                })
            }
        }

        let agent = Arc::new(CapturingAgent {
            id: AgentId::new("a1").unwrap(),
            specialty: specialty(),
            seen_requests: Mutex::new(Vec::new()),
        });
        let council = council_with(&["a1"]);
        let (usecase, _repo, _bus) = fixture(vec![agent.clone() as Arc<dyn AgentPort>], council);

        let external_context = sample_external_context();
        let task = task_with_context(
            TaskConstraints::new(Rubric::empty(), Rounds::new(0).unwrap(), None, None),
            external_context.clone(),
        );

        usecase.execute(task).await.unwrap();

        let seen = agent.seen_requests.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].external_context.as_ref(), Some(&external_context));
    }

    struct PreferInvalidScore;

    #[async_trait]
    impl ScoringPort for PreferInvalidScore {
        async fn score(&self, reports: &[ValidatorReport]) -> Result<Score, DomainError> {
            let all_passed = reports.iter().all(ValidatorReport::passed);
            Score::new(if all_passed { 0.2 } else { 0.9 })
        }
    }

    struct JsonObjectContractValidator;

    #[async_trait]
    impl ValidatorPort for JsonObjectContractValidator {
        fn kind(&self) -> &'static str {
            "json-object"
        }

        async fn validate(
            &self,
            content: &str,
            constraints: &TaskConstraints,
        ) -> Result<ValidatorReport, DomainError> {
            if constraints.output_contract().is_none() {
                return ValidatorReport::new(self.kind(), true, "no contract", Attributes::empty());
            }
            let passed = serde_json::from_str::<serde_json::Value>(content)
                .ok()
                .is_some_and(|value| value.is_object());
            ValidatorReport::new(
                self.kind(),
                passed,
                if passed {
                    "valid object"
                } else {
                    "invalid object"
                },
                Attributes::empty(),
            )
        }
    }

    fn structured_constraints() -> TaskConstraints {
        TaskConstraints::new(Rubric::empty(), Rounds::new(0).unwrap(), None, None)
            .with_output_contract(
                OutputContract::json_object(
                    "decision-contract",
                    BTreeMap::from([(
                        "decision".to_owned(),
                        OutputFieldRule::new(true, ["emit_event", "escalate"]).unwrap(),
                    )]),
                )
                .unwrap(),
            )
    }

    fn fixture_with_scoring(
        agents: Vec<Arc<dyn AgentPort>>,
        council: Council,
        scoring: Arc<dyn ScoringPort>,
        validators: Vec<Arc<dyn ValidatorPort>>,
    ) -> (DeliberateUseCase, Arc<InMemoryRepo>, Arc<NullBus>) {
        build_usecase(
            agents,
            council,
            scoring,
            validators,
            Arc::new(NoopMetricsRecorder),
        )
    }

    #[tokio::test]
    async fn structured_output_mode_prefers_valid_proposal_even_when_invalid_scores_higher() {
        let council = council_with(&["a1", "a2"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("a1", &sp, "plain text", vec![]) as Arc<dyn AgentPort>,
            StubAgent::new("a2", &sp, r#"{"decision":"emit_event"}"#, vec![]) as Arc<dyn AgentPort>,
        ];
        let (usecase, _repo, bus) = fixture_with_scoring(
            agents,
            council,
            Arc::new(PreferInvalidScore),
            vec![Arc::new(JsonObjectContractValidator)],
        );

        let out = usecase
            .execute(task(structured_constraints()))
            .await
            .unwrap();
        let winner_id = out.winner_proposal_id.clone();
        let winner = out.deliberation.proposals().get(&winner_id).unwrap();

        assert_eq!(winner.author().as_str(), "a2");
        assert_eq!(out.deliberation.ranking()[0].as_str(), winner_id.as_str());
        assert_eq!(
            bus.completed.lock().unwrap()[0]
                .winner_proposal_id()
                .as_str(),
            winner_id.as_str()
        );
    }

    #[tokio::test]
    async fn structured_output_mode_fails_when_every_proposal_is_invalid() {
        let council = council_with(&["a1"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> =
            vec![StubAgent::new("a1", &sp, "plain text", vec![]) as Arc<dyn AgentPort>];
        let (usecase, repo, bus) = fixture_with_scoring(
            agents,
            council,
            Arc::new(PreferInvalidScore),
            vec![Arc::new(JsonObjectContractValidator)],
        );

        let err = usecase
            .execute(task(structured_constraints()))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::NoValidProposal { ref contract_id } if contract_id == "decision-contract"
        ));
        assert_eq!(repo.saved.lock().unwrap().len(), 1);
        assert_eq!(bus.completed.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn validator_error_aborts_deliberation_without_saving_or_publishing() {
        let council = council_with(&["a1"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> =
            vec![StubAgent::new("a1", &sp, "a valid proposal", vec![]) as Arc<dyn AgentPort>];
        let (usecase, repo, bus) = fixture_with_scoring(
            agents,
            council,
            Arc::new(LinearScoring),
            vec![Arc::new(FailingValidator)],
        );

        let err = usecase
            .execute(task(TaskConstraints::default()))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvariantViolated { reason } if reason == "validator boom"
        ));
        // A validator error must abort before the deliberation is persisted
        // or any completion event is published — no half-finished state.
        assert!(repo.saved.lock().unwrap().is_empty());
        assert!(bus.completed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn records_success_outcome_winner_score_and_duration() {
        let council = council_with(&["a1", "a2"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("a1", &sp, "a valid proposal", vec![]) as Arc<dyn AgentPort>,
            StubAgent::new("a2", &sp, "another valid proposal", vec![]) as Arc<dyn AgentPort>,
        ];
        let metrics = Arc::new(RecordingMetrics::default());
        let (usecase, _repo, _bus) = fixture_with_metrics(agents, council, metrics.clone());

        usecase
            .execute(task(TaskConstraints::new(
                Rubric::empty(),
                Rounds::new(0).unwrap(),
                None,
                None,
            )))
            .await
            .unwrap();

        // Exactly one success outcome, for the right specialty.
        assert_eq!(
            metrics.outcomes.lock().unwrap().as_slice(),
            &[("reviewer".to_owned(), "success")]
        );
        // A winner score and a duration were observed for the run.
        assert_eq!(metrics.winner_scores.lock().unwrap().len(), 1);
        assert_eq!(metrics.durations.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn records_no_valid_proposal_outcome_without_a_winner_score() {
        let council = council_with(&["a1"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> =
            vec![StubAgent::new("a1", &sp, "plain text", vec![]) as Arc<dyn AgentPort>];
        let metrics = Arc::new(RecordingMetrics::default());
        let (usecase, _repo, _bus) = build_usecase(
            agents,
            council,
            Arc::new(PreferInvalidScore),
            vec![Arc::new(JsonObjectContractValidator)],
            metrics.clone(),
        );

        let err = usecase
            .execute(task(structured_constraints()))
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::NoValidProposal { .. }));

        // The failure outcome is recorded, and no winner score is — there
        // was no winner. The duration is still timed (it precedes the
        // winner selection).
        assert_eq!(
            metrics.outcomes.lock().unwrap().as_slice(),
            &[("reviewer".to_owned(), "no_valid_proposal")]
        );
        assert!(metrics.winner_scores.lock().unwrap().is_empty());
        assert_eq!(metrics.durations.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn records_the_scoring_policys_discrimination() {
        let council = council_with(&["a1", "a2"]);
        let sp = specialty();
        let agents: Vec<Arc<dyn AgentPort>> = vec![
            StubAgent::new("a1", &sp, "a valid proposal", vec![]) as Arc<dyn AgentPort>,
            StubAgent::new("a2", &sp, "another valid proposal", vec![]) as Arc<dyn AgentPort>,
        ];
        let metrics = Arc::new(RecordingMetrics::default());
        let (usecase, _repo, _bus) = build_usecase(
            agents,
            council,
            Arc::new(RerankingScoring),
            vec![Arc::new(ContentLengthValidator)],
            metrics.clone(),
        );

        usecase
            .execute(task(TaskConstraints::new(
                Rubric::empty(),
                Rounds::new(0).unwrap(),
                None,
                None,
            )))
            .await
            .unwrap();

        // The use case forwarded the policy's discrimination verdict.
        assert_eq!(
            metrics.discriminations.lock().unwrap().as_slice(),
            &[("reviewer".to_owned(), "reranked")]
        );
    }
}
