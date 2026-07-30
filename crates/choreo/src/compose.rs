//! Wire every adapter and use case into a runnable [`Application`].

use std::sync::Arc;

use choreo_adapters::agents::DispatchingAgentFactory;
use choreo_adapters::ceremony::DeliberatingCeremonyStepHandler;
use choreo_adapters::clock::SystemClock;
use choreo_adapters::config::EnvConfiguration;
use choreo_adapters::memory::{
    InMemoryAgentRegistry, InMemoryCeremonyDefinitionPublications,
    InMemoryCeremonyDefinitionRepository, InMemoryCeremonyInstanceRepository,
    InMemoryCeremonyTranscriptStore, InMemoryContractRegistry, InMemoryCouncilRegistry,
    InMemoryDeliberationRepository, InMemoryStatistics,
};
use choreo_adapters::metrics::PrometheusMetricsRecorder;
use choreo_adapters::nats::{NatsConfig, NatsMessaging, NatsTriggerSubscriber};
use choreo_adapters::noop::{NoopCeremonyEvidenceSource, NoopExecutor, NoopMessaging};
use choreo_adapters::postgres::{
    PostgresAgentRegistry, PostgresConfig, PostgresCouncilRegistry, PostgresDeliberationRepository,
    PostgresPool, PostgresPoolError, PostgresStatistics,
};
use choreo_adapters::redb::RedbCeremonyStore;
use choreo_adapters::runtime::{
    ExecutorBackendConfig, RuntimeExecutor, RuntimeExecutorConnectError,
};
use choreo_adapters::scoring::{JudgeAwareScoring, UniformScoring};
use choreo_adapters::validators::{
    AllowedStringValuesValidator, BoundedEventShapeValidator, ClaimsEvidenceGroundedValidator,
    ClaimsEvidenceSupportedValidator, ContentNonEmptyValidator, JsonObjectOutputValidator,
    JsonSchemaValidator, RequiredFieldsValidator,
};
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::{
    ApplyCeremonyTransitionUseCase, ApproveCeremonyGuardUseCase, CloseCeremonyInterventionUseCase,
    CollectCeremonyEvidenceUseCase, CreateCouncilUseCase, DeferCeremonyGuardUseCase,
    DeleteCouncilUseCase, DeliberateUseCase, GetCeremonyInstanceUseCase, GetDeliberationUseCase,
    ListCeremonyInstancesUseCase, ListCouncilsUseCase, OrchestrateUseCase,
    PrepareCeremonyParticipantsUseCase, RegisterAgentUseCase, RequestCeremonyInterventionUseCase,
    ResolveCeremonyDefinitionUseCase, RespondToCeremonyInterventionUseCase, RunCeremonyStepUseCase,
    RunCeremonyUseCase, RunCouncilDecisionUseCase, StartCeremonyUseCase,
    StartPublishedCeremonyUseCase, UnregisterAgentUseCase,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    AgentFactoryPort, AgentRegistryPort, AgentResolverPort, CeremonyDefinitionPublicationPort,
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort,
    CeremonyTranscriptStorePort, ConfigurationPort, ContractRegistryPort, CouncilRegistryPort,
    DeliberationRepositoryPort, ExecutorPort, MessagingPort, MetricsRecorderPort, ScoringPort,
    ServiceConfig, StatisticsPort, ValidatorPort,
};
use thiserror::Error;
use tracing::{info, warn};

use crate::seeding::SeedingError;

/// Aggregate of every handle the composition root produces.
pub struct Application {
    pub service_config: ServiceConfig,
    pub agent_registry: Arc<dyn AgentRegistryPort>,
    pub agent_resolver: Arc<dyn AgentResolverPort>,
    pub council_registry: Arc<dyn CouncilRegistryPort>,
    pub contract_registry: Arc<dyn ContractRegistryPort>,
    pub repository: Arc<dyn DeliberationRepositoryPort>,
    pub grpc_service: choreo_adapters::grpc::ChoreographerGrpcService,
    pub nats_subscriber: Option<NatsTriggerSubscriber>,
    pub health_state: crate::health::HealthState,
}

impl std::fmt::Debug for Application {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Application")
            .field("service_config", &self.service_config)
            .field("nats_subscriber_enabled", &self.nats_subscriber.is_some())
            .finish()
    }
}

/// Errors produced while composing the application.
#[derive(Debug, Error)]
pub enum ComposeError {
    #[error("domain error during wiring: {0}")]
    Domain(#[from] DomainError),

    #[error("nats connection failed: {0}")]
    NatsConnect(#[source] async_nats::ConnectError),

    #[error("postgres setup failed: {0}")]
    Postgres(#[from] PostgresPoolError),

    #[error("seeding failed: {0}")]
    Seeding(#[from] SeedingError),

    #[error("runtime executor setup failed: {0}")]
    RuntimeExecutor(#[from] RuntimeExecutorConnectError),

    #[error("ceremony store setup failed: {0}")]
    CeremonyStore(String),
}

/// Pick the scoring policy and wire the optional LLM judge.
///
/// When `judge_from_env` yields a judge it is appended to `validators`,
/// and `JudgeAwareScoring` makes its verdict rank proposals; otherwise
/// scoring is uniform. Fails fast when the judge is enabled but
/// misconfigured.
fn wire_scoring(
    validators: &mut Vec<Arc<dyn ValidatorPort>>,
    metrics: Arc<dyn MetricsRecorderPort>,
) -> Result<Arc<dyn ScoringPort>, DomainError> {
    match choreo_adapters::agents::judge_from_env(metrics.clone())? {
        Some(judge) => {
            validators.push(judge);
            info!("scoring: LLM judge enabled; ranking by judge verdict");
            Ok(Arc::new(JudgeAwareScoring::new().with_metrics(metrics)))
        }
        None => Ok(Arc::new(UniformScoring::new())),
    }
}

/// Wire the full application.
///
/// - Reads [`ServiceConfig`] from the environment.
/// - Builds the in-memory registries plus the configured execution
///   backend. `noop` remains the default; richer executors are
///   selected explicitly by deployment configuration.
/// - When `nats_enabled`, connects to NATS and wires both the
///   outbound `NatsMessaging` and the inbound `NatsTriggerSubscriber`.
///   Otherwise uses [`NoopMessaging`].
/// - Optionally seeds demo councils if `CHOREO_SEED_SPECIALTIES` is
///   set, so an empty deployment is immediately exercisable against
///   the AsyncAPI / gRPC contract.
#[allow(clippy::too_many_lines)]
pub async fn compose() -> Result<Application, ComposeError> {
    let service_config = EnvConfiguration::new().load().await?;

    let clock = Arc::new(SystemClock::new());
    // One Prometheus registry for the whole process, shared between the
    // use cases that record into it and the health endpoint that renders
    // it. Fails fast if a metric is malformed (a wiring bug).
    let metrics_recorder = Arc::new(PrometheusMetricsRecorder::new()?);
    let mut validators: Vec<Arc<dyn ValidatorPort>> = vec![
        Arc::new(ContentNonEmptyValidator::new()),
        Arc::new(JsonObjectOutputValidator::new()),
        Arc::new(RequiredFieldsValidator::new()),
        Arc::new(AllowedStringValuesValidator::new()),
        Arc::new(JsonSchemaValidator::new()),
        // Evidence grounding: rejects claims citing refs outside the
        // contract's evidence pack (no-op unless a contract declares a
        // grounding rule). Runs before the shape-budget guard so orphan
        // refs are named even when the output is otherwise well-formed.
        Arc::new(ClaimsEvidenceGroundedValidator::new()),
        // Semantic support: rejects claims whose *cited* evidence does
        // not actually support them, judged through the deployment's
        // evidence-support judge (`CHOREO_SUPPORT_JUDGE_ENABLED`, vLLM
        // endpoint/model). No-op unless a contract declares
        // `evidence.semantic_support`; a contract that demands it with
        // no judge wired fails its step loudly instead of running the
        // gate voided.
        Arc::new(ClaimsEvidenceSupportedValidator::new(
            choreo_adapters::agents::support_judge_from_env(metrics_recorder.clone())?,
        )),
        // Final shape-budget guard: defends downstream bus consumers
        // against pathological JSON (deeply nested, huge arrays,
        // bloated strings). Uses the validator's conservative
        // defaults; tune with the `with_*` builders when a deploy
        // needs different bounds.
        Arc::new(BoundedEventShapeValidator::new()),
    ];
    // Choose scoring, and when an LLM judge is configured append it to
    // the validator chain so its verdict drives the ranking.
    let scoring: Arc<dyn ScoringPort> = wire_scoring(&mut validators, metrics_recorder.clone())?;
    let executor = wire_executor().await?;
    let dispatching_factory =
        DispatchingAgentFactory::from_env()?.with_metrics(metrics_recorder.clone());
    let supported_agent_kinds = dispatching_factory.supported_kinds().join(",");
    let agent_factory: Arc<dyn AgentFactoryPort> = Arc::new(dispatching_factory);

    // Pick the persistent backings together so the three registries
    // and the deliberation repository always live on the same pool
    // (or all in-memory). Running one Postgres and two in-memory
    // would split the source of truth across replicas.
    let Persistence {
        repository,
        council_registry,
        agent_registry,
        agent_resolver,
        statistics,
        pool: postgres_pool,
    } = wire_persistence(&service_config, agent_factory.clone()).await?;

    // The contract registry is in-memory only today: contracts are
    // small, stable, and seeded from `CHOREO_CONTRACT_DIR` so the
    // operator's source of truth lives on disk. When Postgres-backed
    // contracts land it joins `Persistence` above.
    let contract_registry: Arc<dyn ContractRegistryPort> =
        Arc::new(InMemoryContractRegistry::new());
    let ceremony_definitions: Arc<dyn CeremonyDefinitionRepositoryPort> =
        Arc::new(InMemoryCeremonyDefinitionRepository::new());
    // Ceremony state is durable only when a store path is configured.
    // Leaving it in memory is a valid choice for a throwaway
    // deployment and a silent data-loss bug in any other, so an
    // unconfigured server says what it is giving up rather than
    // discovering it at the first restart.
    // Ceremony state is durable only when a store path is configured.
    // Leaving it in memory is a valid choice for a throwaway
    // deployment and a silent data-loss bug in any other, so an
    // unconfigured server says what it is giving up rather than
    // discovering it at the first restart.
    // One store serves both ports when durable: an instance and the
    // published definition it is bound to have to survive together, or
    // a restart leaves instances pointing at versions that are gone.
    let (ceremony_instances, ceremony_publications): (
        Arc<dyn CeremonyInstanceRepositoryPort>,
        Arc<dyn CeremonyDefinitionPublicationPort>,
    ) = if let Some(path) = service_config.ceremony_store_path.as_deref() {
        let store = Arc::new(
            RedbCeremonyStore::open(path)
                .map_err(|error| ComposeError::CeremonyStore(format!("at {path}: {error}")))?,
        );
        info!(path, "ceremony state is durable");
        (store.clone(), store)
    } else {
        warn!(
            "CHOREO_CEREMONY_STORE_PATH is unset: ceremony state is held in memory. Step \
             leases, idempotency keys and pending human guards will not survive a restart."
        );
        (
            Arc::new(InMemoryCeremonyInstanceRepository::new()),
            Arc::new(InMemoryCeremonyDefinitionPublications::new()),
        )
    };

    let ceremony_transcript_store: Arc<dyn CeremonyTranscriptStorePort> =
        Arc::new(InMemoryCeremonyTranscriptStore::new());

    let MessagingWiring {
        port: messaging,
        subscriber_factory: nats_subscriber_factory,
        nats_client,
    } = wire_messaging(&service_config, metrics_recorder.clone()).await?;

    let deliberate = Arc::new(DeliberateUseCase::new(
        clock.clone(),
        council_registry.clone(),
        agent_resolver.clone(),
        validators,
        scoring,
        repository.clone(),
        messaging.clone(),
        statistics.clone(),
        metrics_recorder.clone(),
        "choreographer",
    ));

    let ceremony_step_handler: Arc<dyn CeremonyStepHandlerPort> =
        Arc::new(DeliberatingCeremonyStepHandler::new(deliberate.clone()));

    let orchestrate = Arc::new(OrchestrateUseCase::new(
        deliberate.clone(),
        executor,
        messaging.clone(),
        clock.clone(),
        statistics.clone(),
        "choreographer",
    ));

    let run_council_decision = Arc::new(RunCouncilDecisionUseCase::new(
        contract_registry.clone(),
        council_registry.clone(),
        deliberate.clone(),
        repository.clone(),
    ));
    let run_ceremony = Arc::new(
        RunCeremonyUseCase::new(
            ceremony_definitions.clone(),
            ceremony_instances.clone(),
            ceremony_step_handler.clone(),
            ceremony_transcript_store.clone(),
            clock.clone(),
        )
        .with_metrics(metrics_recorder.clone()),
    );
    // How every verb that advances a session finds what it is running:
    // from the catalogue when the session is bound to a published
    // version, from the repository when it is not.
    let resolve_ceremony_definition = Arc::new(ResolveCeremonyDefinitionUseCase::new(
        ceremony_definitions.clone(),
        ceremony_publications.clone(),
    ));
    // Advancing a session one move at a time. The transcript store is
    // shared with the whole-run use case above: what a step said has
    // to be there for the next step whichever way the run was driven.
    let start_ceremony = Arc::new(StartCeremonyUseCase::new(
        ceremony_definitions.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let start_published_ceremony = Arc::new(StartPublishedCeremonyUseCase::new(
        ceremony_publications.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let run_ceremony_step = Arc::new(
        RunCeremonyStepUseCase::new(
            resolve_ceremony_definition.clone(),
            ceremony_instances.clone(),
            ceremony_step_handler,
            clock.clone(),
        )
        .with_transcript_store(ceremony_transcript_store),
    );
    let apply_ceremony_transition = Arc::new(ApplyCeremonyTransitionUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let approve_ceremony_guard = Arc::new(ApproveCeremonyGuardUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let defer_ceremony_guard = Arc::new(DeferCeremonyGuardUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let request_ceremony_intervention = Arc::new(RequestCeremonyInterventionUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let respond_to_ceremony_intervention = Arc::new(RespondToCeremonyInterventionUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    let close_ceremony_intervention = Arc::new(CloseCeremonyInterventionUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        clock.clone(),
    ));
    // No evidence source ships with the server, so this answers
    // NOT_FOUND until an operator wires one. Failing plainly beats
    // a missing method or an invented answer.
    let collect_ceremony_evidence = Arc::new(CollectCeremonyEvidenceUseCase::new(
        resolve_ceremony_definition.clone(),
        ceremony_instances.clone(),
        Arc::new(NoopCeremonyEvidenceSource::new()),
        clock.clone(),
    ));

    let create_council = Arc::new(CreateCouncilUseCase::new(
        clock.clone(),
        council_registry.clone(),
        agent_resolver.clone(),
    ));
    let prepare_ceremony_participants = Arc::new(PrepareCeremonyParticipantsUseCase::new(
        clock.clone(),
        agent_factory.clone(),
        agent_registry.clone(),
        council_registry.clone(),
    ));
    let delete_council = Arc::new(DeleteCouncilUseCase::new(council_registry.clone()));
    let list_councils = Arc::new(ListCouncilsUseCase::new(council_registry.clone()));
    let get_deliberation = Arc::new(GetDeliberationUseCase::new(repository.clone()));

    let register_agent = Arc::new(RegisterAgentUseCase::new(
        agent_factory.clone(),
        agent_registry.clone(),
    ));
    let unregister_agent = Arc::new(UnregisterAgentUseCase::new(agent_registry.clone()));

    let auto_dispatch = Arc::new(AutoDispatchService::new(
        deliberate.clone(),
        "Investigate the incoming trigger event.",
    )?);

    // Seeding — keeps the service exercisable on a fresh boot.
    crate::seeding::apply_env_seeding(
        clock.as_ref(),
        agent_registry.as_ref(),
        council_registry.as_ref(),
    )
    .await?;
    crate::seeding::apply_contract_seeding(contract_registry.as_ref()).await?;

    // Now that the auto-dispatch service exists, the subscriber
    // factory can finish wiring.
    let nats_subscriber = nats_subscriber_factory.map(|factory| factory(auto_dispatch.clone()));

    let get_ceremony_instance =
        Arc::new(GetCeremonyInstanceUseCase::new(ceremony_instances.clone()));
    let list_ceremony_instances = Arc::new(ListCeremonyInstancesUseCase::new(
        ceremony_instances.clone(),
    ));

    let grpc_service = choreo_adapters::grpc::ChoreographerGrpcService::builder()
        .deliberate(deliberate)
        .orchestrate(orchestrate)
        .create_council(create_council)
        .delete_council(delete_council)
        .list_councils(list_councils)
        .get_deliberation(get_deliberation)
        .register_agent(register_agent)
        .unregister_agent(unregister_agent)
        .run_council_decision(run_council_decision)
        .run_ceremony(run_ceremony)
        .start_ceremony(start_ceremony)
        .start_published_ceremony(start_published_ceremony)
        .run_ceremony_step(run_ceremony_step)
        .apply_ceremony_transition(apply_ceremony_transition)
        .approve_ceremony_guard(approve_ceremony_guard)
        .defer_ceremony_guard(defer_ceremony_guard)
        .request_ceremony_intervention(request_ceremony_intervention)
        .respond_to_ceremony_intervention(respond_to_ceremony_intervention)
        .close_ceremony_intervention(close_ceremony_intervention)
        .collect_ceremony_evidence(collect_ceremony_evidence)
        .ceremony_definitions(ceremony_definitions.clone())
        .get_ceremony_instance(get_ceremony_instance)
        .list_ceremony_instances(list_ceremony_instances)
        .resolve_ceremony_definition(resolve_ceremony_definition.clone())
        .prepare_ceremony_participants(prepare_ceremony_participants)
        .contract_registry(contract_registry.clone())
        .auto_dispatch(auto_dispatch)
        .statistics(statistics.clone())
        .service_version(env!("CARGO_PKG_VERSION"))
        .build()?;

    let health_state = crate::health::HealthState::new(
        nats_client,
        postgres_pool,
        statistics.clone(),
        metrics_recorder.clone(),
        env!("CARGO_PKG_VERSION"),
    );

    info!(
        grpc_port = service_config.grpc_port,
        http_port = service_config.http_port,
        nats_enabled = service_config.nats_enabled,
        executor_backend = executor_backend_name(),
        agent_kinds = supported_agent_kinds.as_str(),
        trigger_subject = service_config.trigger_subject.as_str(),
        "choreographer wired"
    );

    Ok(Application {
        service_config,
        agent_registry,
        agent_resolver,
        council_registry,
        contract_registry,
        repository,
        grpc_service,
        nats_subscriber,
        health_state,
    })
}

async fn wire_executor() -> Result<Arc<dyn ExecutorPort>, ComposeError> {
    let executor: Arc<dyn ExecutorPort> = match ExecutorBackendConfig::from_env()? {
        ExecutorBackendConfig::Noop => Arc::new(NoopExecutor::new()),
        ExecutorBackendConfig::Runtime(config) => Arc::new(RuntimeExecutor::connect(config).await?),
    };
    Ok(executor)
}

fn executor_backend_name() -> &'static str {
    match ExecutorBackendConfig::from_env() {
        Ok(ExecutorBackendConfig::Runtime(_)) => "runtime",
        _ => "noop",
    }
}

/// Factory closure that produces a [`NatsTriggerSubscriber`] once the
/// application's `AutoDispatchService` has been constructed.
type SubscriberFactory = Box<dyn FnOnce(Arc<AutoDispatchService>) -> NatsTriggerSubscriber>;

/// How long to wait for NATS to be reachable during startup.
///
/// Deployments bring NATS and the choreographer up together (compose,
/// Kubernetes, etc.). Failing fast on the first connection attempt
/// means any transient unavailability forces a restart; a bounded
/// retry is the production-correct behaviour.
const NATS_CONNECT_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

async fn connect_nats_with_retry(
    url: &str,
    total_budget: std::time::Duration,
) -> Result<async_nats::Client, ComposeError> {
    let deadline = std::time::Instant::now() + total_budget;
    let mut last_err: Option<async_nats::ConnectError> = None;
    while std::time::Instant::now() < deadline {
        match async_nats::connect(url).await {
            Ok(client) => return Ok(client),
            Err(err) => {
                tracing::warn!(url, error = %err, "nats not ready yet; retrying");
                last_err = Some(err);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
    Err(ComposeError::NatsConnect(last_err.unwrap_or_else(|| {
        // Unreachable: the loop exits only after at least one attempt.
        panic!("nats connect budget elapsed with no error recorded")
    })))
}

/// Everything `compose` needs out of the messaging wiring phase:
/// the port implementation that use cases talk through, a factory
/// for the inbound subscriber, and — when NATS is wired — a handle
/// to the live client so the health endpoints can probe it.
struct MessagingWiring {
    port: Arc<dyn MessagingPort>,
    subscriber_factory: Option<SubscriberFactory>,
    nats_client: Option<async_nats::Client>,
}

async fn wire_messaging(
    cfg: &ServiceConfig,
    metrics: Arc<dyn MetricsRecorderPort>,
) -> Result<MessagingWiring, ComposeError> {
    if !cfg.nats_enabled {
        info!("nats disabled; using noop messaging");
        let port: Arc<dyn MessagingPort> = Arc::new(NoopMessaging::new());
        return Ok(MessagingWiring {
            port,
            subscriber_factory: None,
            nats_client: None,
        });
    }

    let nats_cfg = NatsConfig::new(&cfg.nats_url, &cfg.publish_prefix, &cfg.trigger_subject)?;
    let client = connect_nats_with_retry(&nats_cfg.url, NATS_CONNECT_BUDGET).await?;
    info!(url = nats_cfg.url.as_str(), "nats connected");

    let port: Arc<dyn MessagingPort> = Arc::new(
        NatsMessaging::new(client.clone(), nats_cfg.subjects.clone()).with_metrics(metrics),
    );

    let subjects = nats_cfg.subjects.clone();
    let factory_client = client.clone();
    let subscriber_factory: SubscriberFactory =
        Box::new(move |dispatch| NatsTriggerSubscriber::new(factory_client, subjects, dispatch));

    Ok(MessagingWiring {
        port,
        subscriber_factory: Some(subscriber_factory),
        nats_client: Some(client),
    })
}

/// Composite of the persistent handles the app needs. Kept as a
/// single bag so the composition root wires them together — either
/// all backed by Postgres, or all in-memory. Splitting the source of
/// truth across replicas (half Postgres, half in-memory) is not a
/// useful configuration today.
struct Persistence {
    repository: Arc<dyn DeliberationRepositoryPort>,
    council_registry: Arc<dyn CouncilRegistryPort>,
    agent_registry: Arc<dyn AgentRegistryPort>,
    agent_resolver: Arc<dyn AgentResolverPort>,
    statistics: Arc<dyn StatisticsPort>,
    /// `Some` when Postgres-backed, so the readiness probe can check the
    /// database; `None` for in-memory persistence.
    pool: Option<PostgresPool>,
}

/// Pick persistent backings based on config. When `CHOREO_POSTGRES_URL`
/// is set, every registry that has a Postgres adapter goes through
/// it; migrations apply on startup so a fresh cluster is exercisable.
/// Otherwise the in-memory defaults are wired.
async fn wire_persistence(
    cfg: &ServiceConfig,
    agent_factory: Arc<dyn AgentFactoryPort>,
) -> Result<Persistence, ComposeError> {
    if let Some(url) = cfg.postgres_url.as_deref() {
        let pool = PostgresPool::connect(&PostgresConfig::from_url(url)).await?;
        pool.run_migrations().await?;
        let agents = Arc::new(PostgresAgentRegistry::new(pool.clone(), agent_factory));
        info!("postgres persistence wired (deliberations, councils, agents, statistics)");
        Ok(Persistence {
            repository: Arc::new(PostgresDeliberationRepository::new(pool.clone())),
            council_registry: Arc::new(PostgresCouncilRegistry::new(pool.clone())),
            agent_registry: agents.clone(),
            agent_resolver: agents,
            statistics: Arc::new(PostgresStatistics::new(pool.clone())),
            pool: Some(pool),
        })
    } else {
        info!("postgres disabled; using in-memory persistence");
        let agents = Arc::new(InMemoryAgentRegistry::new());
        Ok(Persistence {
            repository: Arc::new(InMemoryDeliberationRepository::new()),
            council_registry: Arc::new(InMemoryCouncilRegistry::new()),
            agent_registry: agents.clone(),
            agent_resolver: agents,
            statistics: Arc::new(InMemoryStatistics::new()),
            pool: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use choreo_proto::runtime_v1 as runtime_pb;
    use tonic::{transport::Server, Request, Response, Status};

    // Shared across every test in this module so concurrent CHOREO_*
    // env mutations cannot race each other. Previously each test held
    // its own per-fn static, which serialised the test against itself
    // but did nothing across tests — under cargo's default parallel
    // runner the two `compose_builds_application_*` tests then
    // clobbered each other's vars, producing flaky NATS DNS lookups
    // in CI.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn compose_builds_application_with_nats_disabled() {
        let _guard = ENV_LOCK.lock().await;

        // Clear CHOREO_* so the defaults apply, then disable NATS
        // (so this test does not require a broker).
        for (k, _) in std::env::vars() {
            if k.starts_with("CHOREO_") {
                std::env::remove_var(k);
            }
        }
        std::env::set_var("CHOREO_NATS_ENABLED", "false");

        let app = compose().await.expect("compose should succeed");
        assert!(!app.service_config.nats_enabled);
        assert!(app.nats_subscriber.is_none());
        // The gRPC service is wired and ready but no server has started.
        let _ = &app.grpc_service;

        std::env::remove_var("CHOREO_NATS_ENABLED");
    }

    #[derive(Debug, Clone, Default)]
    struct StubRuntime;

    #[async_trait]
    impl runtime_pb::session_service_server::SessionService for StubRuntime {
        async fn create_session(
            &self,
            _request: Request<runtime_pb::CreateSessionRequest>,
        ) -> Result<Response<runtime_pb::CreateSessionResponse>, Status> {
            Ok(Response::new(runtime_pb::CreateSessionResponse {
                session: None,
            }))
        }

        async fn close_session(
            &self,
            _request: Request<runtime_pb::CloseSessionRequest>,
        ) -> Result<Response<runtime_pb::CloseSessionResponse>, Status> {
            Ok(Response::new(runtime_pb::CloseSessionResponse {
                closed: true,
            }))
        }
    }

    #[async_trait]
    impl runtime_pb::invocation_service_server::InvocationService for StubRuntime {
        async fn invoke_tool(
            &self,
            _request: Request<runtime_pb::InvokeToolRequest>,
        ) -> Result<Response<runtime_pb::InvokeToolResponse>, Status> {
            Ok(Response::new(runtime_pb::InvokeToolResponse {
                invocation: None,
            }))
        }
    }

    async fn spawn_runtime_stub() -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

        tokio::spawn(async move {
            Server::builder()
                .add_service(
                    runtime_pb::session_service_server::SessionServiceServer::new(StubRuntime),
                )
                .add_service(
                    runtime_pb::invocation_service_server::InvocationServiceServer::new(
                        StubRuntime,
                    ),
                )
                .serve_with_incoming_shutdown(incoming, async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        (addr, shutdown_tx)
    }

    #[tokio::test]
    async fn compose_builds_application_with_runtime_executor_selected() {
        let _guard = ENV_LOCK.lock().await;

        for (k, _) in std::env::vars() {
            if k.starts_with("CHOREO_") {
                std::env::remove_var(k);
            }
        }

        let (addr, shutdown) = spawn_runtime_stub().await;
        std::env::set_var("CHOREO_NATS_ENABLED", "false");
        std::env::set_var("CHOREO_EXECUTOR_KIND", "runtime");
        std::env::set_var("CHOREO_RUNTIME_GRPC_ENDPOINT", format!("http://{addr}"));

        let app = compose().await.expect("compose should succeed");
        assert!(!app.service_config.nats_enabled);
        assert!(app.nats_subscriber.is_none());

        let _ = shutdown.send(());
        std::env::remove_var("CHOREO_NATS_ENABLED");
        std::env::remove_var("CHOREO_EXECUTOR_KIND");
        std::env::remove_var("CHOREO_RUNTIME_GRPC_ENDPOINT");
    }
}
