//! Wire every adapter and use case into a runnable [`Application`].

use std::sync::Arc;

use choreo_adapters::clock::SystemClock;
use choreo_adapters::config::EnvConfiguration;
use choreo_adapters::memory::{
    InMemoryAgentRegistry, InMemoryCouncilRegistry, InMemoryDeliberationRepository,
    InMemoryStatistics,
};
use choreo_adapters::nats::{NatsConfig, NatsMessaging, NatsTriggerSubscriber};
use choreo_adapters::noop::{NoopAgentFactory, NoopExecutor, NoopMessaging};
use choreo_adapters::postgres::{
    PostgresConfig, PostgresDeliberationRepository, PostgresPool, PostgresPoolError,
};
use choreo_adapters::scoring::UniformScoring;
use choreo_adapters::validators::ContentNonEmptyValidator;
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::{
    CreateCouncilUseCase, DeleteCouncilUseCase, DeliberateUseCase, GetDeliberationUseCase,
    ListCouncilsUseCase, OrchestrateUseCase, RegisterAgentUseCase, UnregisterAgentUseCase,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    AgentFactoryPort, AgentRegistryPort, ConfigurationPort, DeliberationRepositoryPort,
    ExecutorPort, MessagingPort, ScoringPort, ServiceConfig, StatisticsPort, ValidatorPort,
};
use thiserror::Error;
use tracing::info;

use crate::seeding::SeedingError;

/// Aggregate of every handle the composition root produces.
pub struct Application {
    pub service_config: ServiceConfig,
    pub agent_registry: Arc<InMemoryAgentRegistry>,
    pub council_registry: Arc<InMemoryCouncilRegistry>,
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
}

/// Wire the full application.
///
/// - Reads [`ServiceConfig`] from the environment.
/// - Builds the in-memory registries and the no-op executor /
///   validator / scoring defaults. Operators swap these for richer
///   adapters through later slices (5d providers, real repos, …)
///   without touching this function.
/// - When `nats_enabled`, connects to NATS and wires both the
///   outbound `NatsMessaging` and the inbound `NatsTriggerSubscriber`.
///   Otherwise uses [`NoopMessaging`].
/// - Optionally seeds demo councils if `CHOREO_SEED_SPECIALTIES` is
///   set, so an empty deployment is immediately exercisable against
///   the AsyncAPI / gRPC contract.
pub async fn compose() -> Result<Application, ComposeError> {
    let service_config = EnvConfiguration::new().load().await?;

    let clock = Arc::new(SystemClock::new());
    let agent_registry = Arc::new(InMemoryAgentRegistry::new());
    let council_registry = Arc::new(InMemoryCouncilRegistry::new());
    let repository = wire_repository(&service_config).await?;

    let validators: Vec<Arc<dyn ValidatorPort>> = vec![Arc::new(ContentNonEmptyValidator::new())];
    let scoring: Arc<dyn ScoringPort> = Arc::new(UniformScoring::new());
    let executor: Arc<dyn ExecutorPort> = Arc::new(NoopExecutor::new());

    let MessagingWiring {
        port: messaging,
        subscriber_factory: nats_subscriber_factory,
        nats_client,
    } = wire_messaging(&service_config).await?;

    let statistics: Arc<dyn StatisticsPort> = Arc::new(InMemoryStatistics::new());

    let deliberate = Arc::new(DeliberateUseCase::new(
        clock.clone(),
        council_registry.clone(),
        agent_registry.clone(),
        validators,
        scoring,
        repository.clone(),
        messaging.clone(),
        statistics.clone(),
        "choreographer",
    ));

    let orchestrate = Arc::new(OrchestrateUseCase::new(
        deliberate.clone(),
        executor,
        messaging.clone(),
        clock.clone(),
        statistics.clone(),
        "choreographer",
    ));

    let create_council = Arc::new(CreateCouncilUseCase::new(
        clock.clone(),
        council_registry.clone(),
        agent_registry.clone(),
    ));
    let delete_council = Arc::new(DeleteCouncilUseCase::new(council_registry.clone()));
    let list_councils = Arc::new(ListCouncilsUseCase::new(council_registry.clone()));
    let get_deliberation = Arc::new(GetDeliberationUseCase::new(repository.clone()));

    let agent_factory: Arc<dyn AgentFactoryPort> = Arc::new(NoopAgentFactory::new());
    let agent_registry_port: Arc<dyn AgentRegistryPort> = agent_registry.clone();
    let register_agent = Arc::new(RegisterAgentUseCase::new(
        agent_factory,
        agent_registry_port.clone(),
    ));
    let unregister_agent = Arc::new(UnregisterAgentUseCase::new(agent_registry_port));

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

    // Now that the auto-dispatch service exists, the subscriber
    // factory can finish wiring.
    let nats_subscriber = nats_subscriber_factory.map(|factory| factory(auto_dispatch.clone()));

    let grpc_service = choreo_adapters::grpc::ChoreographerGrpcService::builder()
        .deliberate(deliberate)
        .orchestrate(orchestrate)
        .create_council(create_council)
        .delete_council(delete_council)
        .list_councils(list_councils)
        .get_deliberation(get_deliberation)
        .register_agent(register_agent)
        .unregister_agent(unregister_agent)
        .auto_dispatch(auto_dispatch)
        .statistics(statistics.clone())
        .service_version(env!("CARGO_PKG_VERSION"))
        .build()?;

    let health_state =
        crate::health::HealthState::new(nats_client, statistics.clone(), env!("CARGO_PKG_VERSION"));

    info!(
        grpc_port = service_config.grpc_port,
        http_port = service_config.http_port,
        nats_enabled = service_config.nats_enabled,
        trigger_subject = service_config.trigger_subject.as_str(),
        "choreographer wired"
    );

    Ok(Application {
        service_config,
        agent_registry,
        council_registry,
        repository,
        grpc_service,
        nats_subscriber,
        health_state,
    })
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

async fn wire_messaging(cfg: &ServiceConfig) -> Result<MessagingWiring, ComposeError> {
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

    let port: Arc<dyn MessagingPort> = Arc::new(NatsMessaging::new(
        client.clone(),
        nats_cfg.subjects.clone(),
    ));

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

/// Pick a [`DeliberationRepositoryPort`] implementation based on the
/// environment: Postgres when `CHOREO_POSTGRES_URL` is set (migrations
/// apply on startup so a fresh cluster is exercisable), in-memory
/// otherwise.
async fn wire_repository(
    cfg: &ServiceConfig,
) -> Result<Arc<dyn DeliberationRepositoryPort>, ComposeError> {
    if let Some(url) = cfg.postgres_url.as_deref() {
        let pg_cfg = PostgresConfig::from_url(url);
        let pool = PostgresPool::connect(&pg_cfg).await?;
        pool.run_migrations().await?;
        info!("postgres deliberation repository wired");
        Ok(Arc::new(PostgresDeliberationRepository::new(pool)))
    } else {
        info!("postgres disabled; using in-memory deliberation repository");
        Ok(Arc::new(InMemoryDeliberationRepository::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compose_builds_application_with_nats_disabled() {
        // Serialize env mutations to avoid races with other tests
        // that touch `CHOREO_*` vars (see `config::tests` for the
        // same pattern — we keep a local mutex here so we do not
        // depend on another crate's private state).
        static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
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
}
