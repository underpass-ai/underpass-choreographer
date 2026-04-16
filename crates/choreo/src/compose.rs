//! Wire every adapter and use case into a runnable [`Application`].

use std::sync::Arc;

use choreo_adapters::clock::SystemClock;
use choreo_adapters::config::EnvConfiguration;
use choreo_adapters::memory::{
    InMemoryAgentRegistry, InMemoryCouncilRegistry, InMemoryDeliberationRepository,
};
use choreo_adapters::nats::{NatsConfig, NatsMessaging, NatsTriggerSubscriber};
use choreo_adapters::noop::{NoopExecutor, NoopMessaging};
use choreo_adapters::scoring::UniformScoring;
use choreo_adapters::validators::ContentNonEmptyValidator;
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::{
    CreateCouncilUseCase, DeleteCouncilUseCase, DeliberateUseCase, GetDeliberationUseCase,
    ListCouncilsUseCase, OrchestrateUseCase,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    ConfigurationPort, ExecutorPort, MessagingPort, ScoringPort, ServiceConfig, ValidatorPort,
};
use thiserror::Error;
use tracing::info;

use crate::seeding::SeedingError;

/// Aggregate of every handle the composition root produces.
pub struct Application {
    pub service_config: ServiceConfig,
    pub agent_registry: Arc<InMemoryAgentRegistry>,
    pub council_registry: Arc<InMemoryCouncilRegistry>,
    pub repository: Arc<InMemoryDeliberationRepository>,
    pub grpc_service: choreo_adapters::grpc::ChoreographerGrpcService,
    pub nats_subscriber: Option<NatsTriggerSubscriber>,
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
    let repository = Arc::new(InMemoryDeliberationRepository::new());

    let validators: Vec<Arc<dyn ValidatorPort>> = vec![Arc::new(ContentNonEmptyValidator::new())];
    let scoring: Arc<dyn ScoringPort> = Arc::new(UniformScoring::new());
    let executor: Arc<dyn ExecutorPort> = Arc::new(NoopExecutor::new());

    let (messaging, nats_subscriber_factory) = wire_messaging(&service_config).await?;

    let deliberate = Arc::new(DeliberateUseCase::new(
        clock.clone(),
        council_registry.clone(),
        agent_registry.clone(),
        validators,
        scoring,
        repository.clone(),
        messaging.clone(),
        "choreographer",
    ));

    let orchestrate = Arc::new(OrchestrateUseCase::new(
        deliberate.clone(),
        executor,
        messaging.clone(),
        clock.clone(),
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
        .auto_dispatch(auto_dispatch)
        .build()?;

    info!(
        grpc_port = service_config.grpc_port,
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
    })
}

/// Factory closure that produces a [`NatsTriggerSubscriber`] once the
/// application's `AutoDispatchService` has been constructed.
type SubscriberFactory = Box<dyn FnOnce(Arc<AutoDispatchService>) -> NatsTriggerSubscriber>;

async fn wire_messaging(
    cfg: &ServiceConfig,
) -> Result<(Arc<dyn MessagingPort>, Option<SubscriberFactory>), ComposeError> {
    if !cfg.nats_enabled {
        info!("nats disabled; using noop messaging");
        let messaging: Arc<dyn MessagingPort> = Arc::new(NoopMessaging::new());
        return Ok((messaging, None));
    }

    let nats_cfg = NatsConfig::new(&cfg.nats_url, &cfg.publish_prefix, &cfg.trigger_subject)?;
    let client = async_nats::connect(&nats_cfg.url)
        .await
        .map_err(ComposeError::NatsConnect)?;
    info!(url = nats_cfg.url.as_str(), "nats connected");

    let messaging: Arc<dyn MessagingPort> = Arc::new(NatsMessaging::new(
        client.clone(),
        nats_cfg.subjects.clone(),
    ));

    let subjects = nats_cfg.subjects.clone();
    let factory: SubscriberFactory =
        Box::new(move |dispatch| NatsTriggerSubscriber::new(client, subjects, dispatch));

    Ok((messaging, Some(factory)))
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
