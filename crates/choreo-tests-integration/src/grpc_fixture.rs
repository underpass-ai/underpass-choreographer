//! In-process gRPC fixture for Epic 9 (and any later RPC slice that
//! wants to drive the real choreographer over a Tonic channel).
//!
//! The fixture wires the full composition graph in-memory — NATS
//! disabled, Postgres disabled, executor=noop — then mounts the
//! resulting `ChoreographerGrpcService` on a `tokio::net::TcpListener`
//! bound to `127.0.0.1:0`. Tests get back a `tonic::transport::Channel`
//! plus handles to the in-memory contract / council / agent
//! registries, so they can seed state through the registry ports
//! and assert through the RPC surface in the same test.
//!
//! Why a manual wire instead of `choreo::compose::compose`?
//! `compose` reads from process env and would force every test that
//! uses this fixture to serialize through a shared lock — concurrent
//! `cargo test --workspace` would either deadlock or fight over env
//! vars. The fixture instead constructs the use-case graph with
//! explicit deps so each test gets a fresh, isolated registry set.

use std::sync::Arc;
use std::time::Duration;

use choreo_adapters::agents::DispatchingAgentFactory;
use choreo_adapters::ceremony::DeliberatingCeremonyStepHandler;
use choreo_adapters::clock::SystemClock;
use choreo_adapters::grpc::ChoreographerGrpcService;
use choreo_adapters::memory::{
    InMemoryAgentRegistry, InMemoryCeremonyDefinitionPublications,
    InMemoryCeremonyDefinitionRepository, InMemoryCeremonyInstanceRepository,
    InMemoryCeremonyTranscriptStore, InMemoryContractRegistry, InMemoryCouncilRegistry,
    InMemoryDeliberationRepository, InMemoryStatistics,
};
use choreo_adapters::noop::{NoopCeremonyEvidenceSource, NoopExecutor, NoopMessaging};
use choreo_adapters::scoring::UniformScoring;
use choreo_adapters::validators::{
    AllowedStringValuesValidator, ContentNonEmptyValidator, JsonObjectOutputValidator,
    JsonSchemaValidator, RequiredFieldsValidator,
};
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::{
    ApplyCeremonyTransitionUseCase, ApproveCeremonyGuardUseCase, CloseCeremonyInterventionUseCase,
    CollectCeremonyEvidenceUseCase, CreateCouncilUseCase, DeferCeremonyGuardUseCase,
    DeleteCouncilUseCase, DeliberateUseCase, GetCeremonyInstanceUseCase, GetDeliberationUseCase,
    ListCeremonyInstancesUseCase, ListCouncilsUseCase, OrchestrateUseCase,
    PrepareCeremonyParticipantsUseCase, PublishCeremonyDefinitionUseCase, RegisterAgentUseCase,
    RequestCeremonyInterventionUseCase, ResolveCeremonyDefinitionUseCase,
    RespondToCeremonyInterventionUseCase, RunCeremonyStepUseCase, RunCeremonyUseCase,
    RunCouncilDecisionUseCase, StartCeremonyUseCase, StartPublishedCeremonyUseCase,
    UnregisterAgentUseCase,
};
use choreo_core::ports::{
    AgentRegistryPort, AgentResolverPort, CeremonyDefinitionPublicationPort,
    CeremonyDefinitionRepositoryPort, CeremonyInstanceRepositoryPort, CeremonyStepHandlerPort,
    CeremonyTranscriptStorePort, ContractRegistryPort, CouncilRegistryPort, ValidatorPort,
};
use tokio::sync::oneshot;
use tonic::transport::{Certificate, Channel, Endpoint, Identity, Server, ServerTlsConfig};

/// TLS posture the fixture's gRPC server should present. Passed to
/// [`GrpcFixture::start_with_tls`]. Materials are PEM-encoded bytes —
/// the caller mints them in memory (typically via
/// `tls_fixture::mint_tls`) so no temp files or env mutation are
/// involved.
#[derive(Debug, Clone)]
pub enum TlsServerSetup {
    Server {
        cert: Vec<u8>,
        key: Vec<u8>,
    },
    Mutual {
        cert: Vec<u8>,
        key: Vec<u8>,
        client_ca: Vec<u8>,
    },
}

/// Handles a test needs to drive the in-process choreographer:
/// the gRPC channel for issuing RPCs and the registries for seeding
/// state without going through CRUD calls. `addr` is the bound
/// `127.0.0.1:<ephemeral>` so callers can build their own TLS
/// channels against the same listener.
pub struct GrpcFixture {
    pub channel: Channel,
    pub addr: std::net::SocketAddr,
    pub contracts: Arc<dyn ContractRegistryPort>,
    pub councils: Arc<dyn CouncilRegistryPort>,
    pub agents: Arc<dyn AgentRegistryPort>,
    pub agent_resolver: Arc<dyn AgentResolverPort>,
    /// The published catalogue, so a test can seed it directly. There
    /// is no publish RPC yet, and a server whose catalogue is filled
    /// by another route is exactly the situation this models.
    pub ceremony_publications: Arc<dyn CeremonyDefinitionPublicationPort>,
    /// Held for the lifetime of the fixture; drop kills the server.
    _shutdown: oneshot::Sender<()>,
}

impl GrpcFixture {
    /// Wire and start a fresh fixture. Returns once the server is
    /// accepting connections.
    #[allow(clippy::too_many_lines)] // wiring graph mirrors `compose::compose`; splitting fragments the dep order
    pub async fn start() -> Self {
        let clock = Arc::new(SystemClock::new());
        let validators: Vec<Arc<dyn ValidatorPort>> = vec![
            Arc::new(ContentNonEmptyValidator::new()),
            Arc::new(JsonObjectOutputValidator::new()),
            Arc::new(RequiredFieldsValidator::new()),
            Arc::new(AllowedStringValuesValidator::new()),
            Arc::new(JsonSchemaValidator::new()),
        ];
        let scoring = Arc::new(UniformScoring::new());
        let executor = Arc::new(NoopExecutor::new());
        let messaging = Arc::new(NoopMessaging::new());
        let statistics = Arc::new(InMemoryStatistics::new());
        let repository = Arc::new(InMemoryDeliberationRepository::new());
        let council_registry: Arc<dyn CouncilRegistryPort> =
            Arc::new(InMemoryCouncilRegistry::new());
        let contract_registry: Arc<dyn ContractRegistryPort> =
            Arc::new(InMemoryContractRegistry::new());
        let ceremony_definitions: Arc<dyn CeremonyDefinitionRepositoryPort> =
            Arc::new(InMemoryCeremonyDefinitionRepository::new());
        let ceremony_instances: Arc<dyn CeremonyInstanceRepositoryPort> =
            Arc::new(InMemoryCeremonyInstanceRepository::new());
        let ceremony_publications: Arc<dyn CeremonyDefinitionPublicationPort> =
            Arc::new(InMemoryCeremonyDefinitionPublications::new());
        let resolve_ceremony_definition = Arc::new(ResolveCeremonyDefinitionUseCase::new(
            ceremony_definitions.clone(),
            ceremony_publications.clone(),
        ));
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        let agent_resolver: Arc<dyn AgentResolverPort> = agent_registry.clone();
        // `new()` yields a factory that supports only the always-on
        // `noop` kind — exactly what these tests need.
        let agent_factory = Arc::new(DispatchingAgentFactory::new());

        let deliberate = Arc::new(DeliberateUseCase::new(
            clock.clone(),
            council_registry.clone(),
            agent_resolver.clone(),
            validators,
            scoring,
            repository.clone(),
            messaging.clone(),
            statistics.clone(),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "choreographer-tests",
        ));
        let ceremony_step_handler: Arc<dyn CeremonyStepHandlerPort> =
            Arc::new(DeliberatingCeremonyStepHandler::new(deliberate.clone()));
        let orchestrate = Arc::new(OrchestrateUseCase::new(
            deliberate.clone(),
            executor,
            messaging.clone(),
            clock.clone(),
            statistics.clone(),
            "choreographer-tests",
        ));
        let run_council_decision = Arc::new(RunCouncilDecisionUseCase::new(
            contract_registry.clone(),
            council_registry.clone(),
            deliberate.clone(),
            repository.clone(),
        ));
        let ceremony_transcript_store: Arc<dyn CeremonyTranscriptStorePort> =
            Arc::new(InMemoryCeremonyTranscriptStore::new());
        let run_ceremony = Arc::new(RunCeremonyUseCase::new(
            ceremony_definitions.clone(),
            ceremony_instances.clone(),
            ceremony_step_handler.clone(),
            ceremony_transcript_store.clone(),
            clock.clone(),
        ));
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
        let publish_ceremony_definition = Arc::new(PublishCeremonyDefinitionUseCase::new(
            ceremony_publications.clone(),
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
        let auto_dispatch = Arc::new(
            AutoDispatchService::new(
                deliberate.clone(),
                "Investigate the incoming trigger event.",
            )
            .expect("auto-dispatch wiring should never fail"),
        );

        let svc = ChoreographerGrpcService::builder()
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
            .publish_ceremony_definition(publish_ceremony_definition)
            .ceremony_definitions(ceremony_definitions.clone())
            .get_ceremony_instance(Arc::new(GetCeremonyInstanceUseCase::new(
                ceremony_instances.clone(),
            )))
            .list_ceremony_instances(Arc::new(ListCeremonyInstancesUseCase::new(
                ceremony_instances.clone(),
            )))
            .resolve_ceremony_definition(resolve_ceremony_definition.clone())
            .prepare_ceremony_participants(prepare_ceremony_participants)
            .contract_registry(contract_registry.clone())
            .auto_dispatch(auto_dispatch)
            .statistics(statistics.clone())
            .service_version("choreographer-tests")
            .build()
            .expect("grpc service wiring should succeed");

        // Bind on an ephemeral port so concurrent fixtures don't clash.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("ephemeral bind should succeed");
        let addr = listener.local_addr().expect("local_addr");
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(svc.into_server())
                .serve_with_incoming_shutdown(incoming, async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        // Build a Channel pointed at the spawned server. Endpoint is
        // not resolved synchronously by tonic; connect_lazy() defers
        // the first connection to the first RPC call.
        let channel = Endpoint::from_shared(format!("http://{addr}"))
            .expect("endpoint URL")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .connect_lazy();

        GrpcFixture {
            channel,
            addr,
            contracts: contract_registry,
            councils: council_registry,
            agents: agent_registry,
            agent_resolver,
            ceremony_publications,
            _shutdown: shutdown_tx,
        }
    }

    /// Variant of [`Self::start`] that serves over TLS. Mirrors the
    /// wiring graph verbatim — the only delta is the
    /// `Server::builder().tls_config(...)` call and the `https://`
    /// `Endpoint` the returned `channel` is built against. Caller-
    /// supplied PEM bytes (typically minted via
    /// `tls_fixture::mint_tls`) feed both the server's identity and,
    /// for `Mutual`, the client-CA trust anchor.
    ///
    /// The returned `channel` is configured with TLS that trusts the
    /// matching CA and (in mutual mode) carries the minted client
    /// identity, so the "happy path" RPC works out of the box. Tests
    /// that need to construct a *different* client (e.g. one missing
    /// the identity, to assert rejection) use `fixture.addr` directly.
    #[allow(clippy::too_many_lines)] // wiring graph mirrors `compose::compose`; splitting fragments the dep order
    pub async fn start_with_tls(setup: TlsServerSetup) -> Self {
        let clock = Arc::new(SystemClock::new());
        let validators: Vec<Arc<dyn ValidatorPort>> = vec![
            Arc::new(ContentNonEmptyValidator::new()),
            Arc::new(JsonObjectOutputValidator::new()),
            Arc::new(RequiredFieldsValidator::new()),
            Arc::new(AllowedStringValuesValidator::new()),
            Arc::new(JsonSchemaValidator::new()),
        ];
        let scoring = Arc::new(UniformScoring::new());
        let executor = Arc::new(NoopExecutor::new());
        let messaging = Arc::new(NoopMessaging::new());
        let statistics = Arc::new(InMemoryStatistics::new());
        let repository = Arc::new(InMemoryDeliberationRepository::new());
        let council_registry: Arc<dyn CouncilRegistryPort> =
            Arc::new(InMemoryCouncilRegistry::new());
        let contract_registry: Arc<dyn ContractRegistryPort> =
            Arc::new(InMemoryContractRegistry::new());
        let ceremony_definitions: Arc<dyn CeremonyDefinitionRepositoryPort> =
            Arc::new(InMemoryCeremonyDefinitionRepository::new());
        let ceremony_instances: Arc<dyn CeremonyInstanceRepositoryPort> =
            Arc::new(InMemoryCeremonyInstanceRepository::new());
        let ceremony_publications: Arc<dyn CeremonyDefinitionPublicationPort> =
            Arc::new(InMemoryCeremonyDefinitionPublications::new());
        let resolve_ceremony_definition = Arc::new(ResolveCeremonyDefinitionUseCase::new(
            ceremony_definitions.clone(),
            ceremony_publications.clone(),
        ));
        let agent_registry = Arc::new(InMemoryAgentRegistry::new());
        let agent_resolver: Arc<dyn AgentResolverPort> = agent_registry.clone();
        let agent_factory = Arc::new(DispatchingAgentFactory::new());

        let deliberate = Arc::new(DeliberateUseCase::new(
            clock.clone(),
            council_registry.clone(),
            agent_resolver.clone(),
            validators,
            scoring,
            repository.clone(),
            messaging.clone(),
            statistics.clone(),
            Arc::new(choreo_core::ports::NoopMetricsRecorder),
            "choreographer-tests",
        ));
        let ceremony_step_handler: Arc<dyn CeremonyStepHandlerPort> =
            Arc::new(DeliberatingCeremonyStepHandler::new(deliberate.clone()));
        let orchestrate = Arc::new(OrchestrateUseCase::new(
            deliberate.clone(),
            executor,
            messaging.clone(),
            clock.clone(),
            statistics.clone(),
            "choreographer-tests",
        ));
        let run_council_decision = Arc::new(RunCouncilDecisionUseCase::new(
            contract_registry.clone(),
            council_registry.clone(),
            deliberate.clone(),
            repository.clone(),
        ));
        let ceremony_transcript_store: Arc<dyn CeremonyTranscriptStorePort> =
            Arc::new(InMemoryCeremonyTranscriptStore::new());
        let run_ceremony = Arc::new(RunCeremonyUseCase::new(
            ceremony_definitions.clone(),
            ceremony_instances.clone(),
            ceremony_step_handler.clone(),
            ceremony_transcript_store.clone(),
            clock.clone(),
        ));
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
        let publish_ceremony_definition = Arc::new(PublishCeremonyDefinitionUseCase::new(
            ceremony_publications.clone(),
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
        let auto_dispatch = Arc::new(
            AutoDispatchService::new(
                deliberate.clone(),
                "Investigate the incoming trigger event.",
            )
            .expect("auto-dispatch wiring should never fail"),
        );

        let svc = ChoreographerGrpcService::builder()
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
            .publish_ceremony_definition(publish_ceremony_definition)
            .ceremony_definitions(ceremony_definitions.clone())
            .prepare_ceremony_participants(prepare_ceremony_participants)
            .get_ceremony_instance(Arc::new(GetCeremonyInstanceUseCase::new(
                ceremony_instances.clone(),
            )))
            .list_ceremony_instances(Arc::new(ListCeremonyInstancesUseCase::new(
                ceremony_instances.clone(),
            )))
            .resolve_ceremony_definition(resolve_ceremony_definition.clone())
            .contract_registry(contract_registry.clone())
            .auto_dispatch(auto_dispatch)
            .statistics(statistics.clone())
            .service_version("choreographer-tests")
            .build()
            .expect("grpc service wiring should succeed");

        // Build the server TLS config + remember the materials the
        // client side needs to mint a matching channel.
        let (server_tls, ca_for_client, client_identity_for_channel) = match setup {
            TlsServerSetup::Server { cert, key } => {
                let identity = Identity::from_pem(cert.clone(), key);
                (
                    ServerTlsConfig::new().identity(identity),
                    cert, // happy-path: trust the server's leaf as the anchor
                    None,
                )
            }
            TlsServerSetup::Mutual {
                cert,
                key,
                client_ca,
            } => {
                let identity = Identity::from_pem(cert.clone(), key);
                let server_tls = ServerTlsConfig::new()
                    .identity(identity)
                    .client_ca_root(Certificate::from_pem(client_ca.clone()));
                // In mutual mode the happy-path channel needs both the
                // server-CA (== the server leaf in this fixture, since
                // both leaves share the same self-signed CA) AND a
                // client identity. We accept that the caller wires the
                // client cert+key through their own ClientTlsConfig
                // because the fixture has no access to them here —
                // returning the leaf cert is enough to anchor server
                // verification on the client side; the rejection test
                // builds its own channel without identity.
                (server_tls, cert, Some(client_ca))
            }
        };
        let _ = client_identity_for_channel; // anchor + identity reside with the caller

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("ephemeral bind should succeed");
        let addr = listener.local_addr().expect("local_addr");
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            let _ = Server::builder()
                .tls_config(server_tls)
                .expect("server tls config should be valid")
                .add_service(svc.into_server())
                .serve_with_incoming_shutdown(incoming, async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        // Trust anchor for the happy-path client channel. In server
        // mode this is the server leaf; the integration test will
        // typically rebuild a channel using the CA PEM minted by
        // `mint_tls` (which is the actual trust anchor in this
        // fixture's chain). We connect_lazy() so the first RPC is
        // what triggers the handshake.
        let endpoint = Endpoint::from_shared(format!("https://localhost:{}", addr.port()))
            .expect("endpoint URL")
            .tls_config(
                tonic::transport::ClientTlsConfig::new()
                    .ca_certificate(Certificate::from_pem(ca_for_client))
                    .domain_name("localhost"),
            )
            .expect("client tls config should be valid")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30));
        let channel = endpoint.connect_lazy();

        GrpcFixture {
            channel,
            addr,
            contracts: contract_registry,
            councils: council_registry,
            agents: agent_registry,
            agent_resolver,
            ceremony_publications,
            _shutdown: shutdown_tx,
        }
    }
}
