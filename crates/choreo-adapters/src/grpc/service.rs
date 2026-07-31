//! gRPC service handler — thin translation from proto RPCs onto
//! use cases in [`choreo_app`].

use std::sync::Arc;

use async_trait::async_trait;
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::{
    ApplyCeremonyTransitionUseCase, ApproveCeremonyGuardUseCase, BindCeremonyParticipantsUseCase,
    CeremonyDraftView, CeremonyInstanceView, CloseCeremonyInterventionUseCase,
    CollectCeremonyEvidenceUseCase, CreateCouncilInput, CreateCouncilUseCase,
    DeferCeremonyGuardUseCase, DeleteCouncilUseCase, DeliberateUseCase,
    DiffCeremonyDefinitionsUseCase, GetCeremonyInstanceUseCase, GetDeliberationUseCase,
    ListCeremonyInstancesUseCase, ListCouncilsUseCase, OrchestrateUseCase,
    PrepareCeremonyParticipantsUseCase, PublishCeremonyDefinitionUseCase, RegisterAgentUseCase,
    RequestCeremonyInterventionUseCase, ResolveCeremonyDefinitionUseCase,
    RespondToCeremonyInterventionUseCase, RunCeremonyStepUseCase, RunCeremonyUseCase,
    RunCouncilDecisionUseCase, StartCeremonyUseCase, StartPublishedCeremonyUseCase,
    UnregisterAgentUseCase,
};
use choreo_core::error::DomainError;
use choreo_core::ports::{
    AgentDescriptor, CeremonyDefinitionRepositoryPort, ContractRegistryPort, StatisticsPort,
};
use choreo_core::value_objects::{AgentId, AgentKind, CeremonyId, Specialty, TaskId};
use choreo_proto::v1 as pb;
use choreo_proto::v1::choreographer_service_server::{
    ChoreographerService, ChoreographerServiceServer,
};
use tonic::{Request, Response, Status};
use tracing::debug;

use super::mappers::{
    apply_ceremony_transition_input_from_proto, approve_ceremony_guard_input_from_proto,
    bind_ceremony_participants_input_from_proto, ceremony_definition_source_from_proto,
    ceremony_instance_state_from, close_ceremony_intervention_input_from_proto,
    collect_ceremony_evidence_input_from_proto, council_summary_from,
    defer_ceremony_guard_input_from_proto, deliberate_response_from,
    diff_ceremony_definitions_response_from, explain_ceremony_draft_response_from,
    orchestrate_response_from, output_contract_from_proto, output_contract_to_proto,
    publish_ceremony_definition_response_from, request_ceremony_intervention_input_from_proto,
    respond_to_ceremony_intervention_input_from_proto, run_ceremony_input_from_proto,
    run_ceremony_response_from, run_ceremony_step_input_from_proto,
    run_council_decision_input_from_proto, run_council_decision_response_from,
    start_ceremony_from_proto, start_published_ceremony_input_from_proto, task_from_proto,
    trigger_event_from_proto, validate_ceremony_draft_response_from, StartCeremonyFromYaml,
};
use super::status::domain_error_to_status;
use super::tracecontext::link_span_to_metadata;
use crate::ceremony::CeremonyParticipantPlanAdapter;
use crate::yaml::CeremonyDefinitionYaml;

/// The gRPC service struct. Clone-friendly: every dependency is an
/// `Arc` so multiple request tasks can share state without locking.
#[derive(Clone)]
pub struct ChoreographerGrpcService {
    deliberate: Arc<DeliberateUseCase>,
    orchestrate: Arc<OrchestrateUseCase>,
    create_council: Arc<CreateCouncilUseCase>,
    delete_council: Arc<DeleteCouncilUseCase>,
    list_councils: Arc<ListCouncilsUseCase>,
    get_deliberation: Arc<GetDeliberationUseCase>,
    register_agent: Arc<RegisterAgentUseCase>,
    unregister_agent: Arc<UnregisterAgentUseCase>,
    run_council_decision: Arc<RunCouncilDecisionUseCase>,
    run_ceremony: Arc<RunCeremonyUseCase>,
    get_ceremony_instance: Arc<GetCeremonyInstanceUseCase>,
    list_ceremony_instances: Arc<ListCeremonyInstancesUseCase>,
    resolve_ceremony_definition: Arc<ResolveCeremonyDefinitionUseCase>,
    start_ceremony: Arc<StartCeremonyUseCase>,
    start_published_ceremony: Arc<StartPublishedCeremonyUseCase>,
    run_ceremony_step: Arc<RunCeremonyStepUseCase>,
    apply_ceremony_transition: Arc<ApplyCeremonyTransitionUseCase>,
    approve_ceremony_guard: Arc<ApproveCeremonyGuardUseCase>,
    defer_ceremony_guard: Arc<DeferCeremonyGuardUseCase>,
    request_ceremony_intervention: Arc<RequestCeremonyInterventionUseCase>,
    respond_to_ceremony_intervention: Arc<RespondToCeremonyInterventionUseCase>,
    close_ceremony_intervention: Arc<CloseCeremonyInterventionUseCase>,
    collect_ceremony_evidence: Arc<CollectCeremonyEvidenceUseCase>,
    diff_ceremony_definitions: Arc<DiffCeremonyDefinitionsUseCase>,
    bind_ceremony_participants: Arc<BindCeremonyParticipantsUseCase>,
    publish_ceremony_definition: Arc<PublishCeremonyDefinitionUseCase>,
    ceremony_definitions: Arc<dyn CeremonyDefinitionRepositoryPort>,
    prepare_ceremony_participants: Arc<PrepareCeremonyParticipantsUseCase>,
    contract_registry: Arc<dyn ContractRegistryPort>,
    auto_dispatch: Arc<AutoDispatchService>,
    statistics: Arc<dyn StatisticsPort>,
    started_at: std::time::Instant,
    service_version: &'static str,
}

impl std::fmt::Debug for ChoreographerGrpcService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChoreographerGrpcService").finish()
    }
}

impl ChoreographerGrpcService {
    #[must_use]
    pub fn builder() -> ChoreographerGrpcServiceBuilder {
        ChoreographerGrpcServiceBuilder::default()
    }

    /// Resolve an instance's definition and derive the view every
    /// transport renders.
    ///
    /// The definition is resolved through the shared use case, so a
    /// bound instance is checked against the digest it recorded here
    /// exactly as it is in the embedded distribution.
    async fn project(
        &self,
        instance: &choreo_core::entities::CeremonyInstance,
    ) -> Result<pb::CeremonyInstanceState, Status> {
        let definition = self
            .resolve_ceremony_definition
            .execute(instance)
            .await
            .map_err(domain_error_to_status)?;
        Self::render(instance, &definition).map_err(domain_error_to_status)
    }

    /// Rendering a session whose definition is already in hand. A move
    /// changes the instance and never the definition, so the mutating
    /// RPCs resolve once and render with what they resolved.
    fn render(
        instance: &choreo_core::entities::CeremonyInstance,
        definition: &choreo_core::entities::CeremonyDefinition,
    ) -> Result<pb::CeremonyInstanceState, DomainError> {
        let view = CeremonyInstanceView::project(instance, definition)?;
        Ok(ceremony_instance_state_from(&view))
    }

    /// Give the session the participants its steps will deliberate
    /// with. RunCeremony does this before it runs; a session advanced
    /// one call at a time needs it just as much, and needs it once, at
    /// the start — otherwise a ceremony can be opened and then never
    /// moved, which is the worst of the two failures.
    async fn prepare_participants(
        &self,
        definition: &choreo_core::entities::CeremonyDefinition,
    ) -> Result<(), Status> {
        let plan = CeremonyParticipantPlanAdapter::from_definition(definition)
            .map_err(domain_error_to_status)?;
        self.prepare_ceremony_participants
            .execute(plan)
            .await
            .map_err(domain_error_to_status)?;
        Ok(())
    }

    /// Load a session together with the definition it runs — the first
    /// thing every move needs and the only place the two are paired.
    async fn session(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Result<
        (
            choreo_core::entities::CeremonyInstance,
            choreo_core::entities::CeremonyDefinition,
        ),
        Status,
    > {
        let instance = self
            .get_ceremony_instance
            .execute(ceremony_id)
            .await
            .map_err(domain_error_to_status)?;
        let definition = self
            .resolve_ceremony_definition
            .execute(&instance)
            .await
            .map_err(domain_error_to_status)?;
        Ok((instance, definition))
    }

    /// Wrap this service into a Tonic `Server` middleware.
    #[must_use]
    pub fn into_server(self) -> ChoreographerServiceServer<Self> {
        ChoreographerServiceServer::new(self)
    }
}

/// Builder so composition-root wiring is readable even as the number
/// of use cases grows.
#[derive(Default)]
pub struct ChoreographerGrpcServiceBuilder {
    deliberate: Option<Arc<DeliberateUseCase>>,
    orchestrate: Option<Arc<OrchestrateUseCase>>,
    create_council: Option<Arc<CreateCouncilUseCase>>,
    delete_council: Option<Arc<DeleteCouncilUseCase>>,
    list_councils: Option<Arc<ListCouncilsUseCase>>,
    get_deliberation: Option<Arc<GetDeliberationUseCase>>,
    register_agent: Option<Arc<RegisterAgentUseCase>>,
    unregister_agent: Option<Arc<UnregisterAgentUseCase>>,
    run_council_decision: Option<Arc<RunCouncilDecisionUseCase>>,
    run_ceremony: Option<Arc<RunCeremonyUseCase>>,
    get_ceremony_instance: Option<Arc<GetCeremonyInstanceUseCase>>,
    list_ceremony_instances: Option<Arc<ListCeremonyInstancesUseCase>>,
    resolve_ceremony_definition: Option<Arc<ResolveCeremonyDefinitionUseCase>>,
    start_ceremony: Option<Arc<StartCeremonyUseCase>>,
    start_published_ceremony: Option<Arc<StartPublishedCeremonyUseCase>>,
    run_ceremony_step: Option<Arc<RunCeremonyStepUseCase>>,
    apply_ceremony_transition: Option<Arc<ApplyCeremonyTransitionUseCase>>,
    approve_ceremony_guard: Option<Arc<ApproveCeremonyGuardUseCase>>,
    defer_ceremony_guard: Option<Arc<DeferCeremonyGuardUseCase>>,
    request_ceremony_intervention: Option<Arc<RequestCeremonyInterventionUseCase>>,
    respond_to_ceremony_intervention: Option<Arc<RespondToCeremonyInterventionUseCase>>,
    close_ceremony_intervention: Option<Arc<CloseCeremonyInterventionUseCase>>,
    collect_ceremony_evidence: Option<Arc<CollectCeremonyEvidenceUseCase>>,
    diff_ceremony_definitions: Option<Arc<DiffCeremonyDefinitionsUseCase>>,
    bind_ceremony_participants: Option<Arc<BindCeremonyParticipantsUseCase>>,
    publish_ceremony_definition: Option<Arc<PublishCeremonyDefinitionUseCase>>,
    ceremony_definitions: Option<Arc<dyn CeremonyDefinitionRepositoryPort>>,
    prepare_ceremony_participants: Option<Arc<PrepareCeremonyParticipantsUseCase>>,
    contract_registry: Option<Arc<dyn ContractRegistryPort>>,
    auto_dispatch: Option<Arc<AutoDispatchService>>,
    statistics: Option<Arc<dyn StatisticsPort>>,
    service_version: Option<&'static str>,
}

impl std::fmt::Debug for ChoreographerGrpcServiceBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChoreographerGrpcServiceBuilder").finish()
    }
}

macro_rules! required {
    ($self:ident, $field:ident) => {
        required!($self, $field, "use case")
    };
    ($self:ident, $field:ident, $what:literal) => {
        $self.$field.ok_or(DomainError::InvariantViolated {
            reason: concat!("grpc: ", stringify!($field), " ", $what, " is required"),
        })?
    };
}

macro_rules! setter {
    ($name:ident, $ty:ty, $field:ident) => {
        #[must_use]
        pub fn $name(mut self, value: Arc<$ty>) -> Self {
            self.$field = Some(value);
            self
        }
    };
}

impl ChoreographerGrpcServiceBuilder {
    setter!(deliberate, DeliberateUseCase, deliberate);
    setter!(orchestrate, OrchestrateUseCase, orchestrate);
    setter!(create_council, CreateCouncilUseCase, create_council);
    setter!(delete_council, DeleteCouncilUseCase, delete_council);
    setter!(list_councils, ListCouncilsUseCase, list_councils);
    setter!(get_deliberation, GetDeliberationUseCase, get_deliberation);
    setter!(register_agent, RegisterAgentUseCase, register_agent);
    setter!(unregister_agent, UnregisterAgentUseCase, unregister_agent);
    setter!(
        run_council_decision,
        RunCouncilDecisionUseCase,
        run_council_decision
    );
    setter!(run_ceremony, RunCeremonyUseCase, run_ceremony);
    setter!(
        get_ceremony_instance,
        GetCeremonyInstanceUseCase,
        get_ceremony_instance
    );
    setter!(
        list_ceremony_instances,
        ListCeremonyInstancesUseCase,
        list_ceremony_instances
    );
    setter!(
        resolve_ceremony_definition,
        ResolveCeremonyDefinitionUseCase,
        resolve_ceremony_definition
    );
    setter!(start_ceremony, StartCeremonyUseCase, start_ceremony);
    setter!(
        start_published_ceremony,
        StartPublishedCeremonyUseCase,
        start_published_ceremony
    );
    setter!(run_ceremony_step, RunCeremonyStepUseCase, run_ceremony_step);
    setter!(
        apply_ceremony_transition,
        ApplyCeremonyTransitionUseCase,
        apply_ceremony_transition
    );
    setter!(
        approve_ceremony_guard,
        ApproveCeremonyGuardUseCase,
        approve_ceremony_guard
    );
    setter!(
        defer_ceremony_guard,
        DeferCeremonyGuardUseCase,
        defer_ceremony_guard
    );
    setter!(
        request_ceremony_intervention,
        RequestCeremonyInterventionUseCase,
        request_ceremony_intervention
    );
    setter!(
        respond_to_ceremony_intervention,
        RespondToCeremonyInterventionUseCase,
        respond_to_ceremony_intervention
    );
    setter!(
        close_ceremony_intervention,
        CloseCeremonyInterventionUseCase,
        close_ceremony_intervention
    );
    setter!(
        collect_ceremony_evidence,
        CollectCeremonyEvidenceUseCase,
        collect_ceremony_evidence
    );
    setter!(
        prepare_ceremony_participants,
        PrepareCeremonyParticipantsUseCase,
        prepare_ceremony_participants
    );
    setter!(
        bind_ceremony_participants,
        BindCeremonyParticipantsUseCase,
        bind_ceremony_participants
    );
    setter!(
        diff_ceremony_definitions,
        DiffCeremonyDefinitionsUseCase,
        diff_ceremony_definitions
    );
    setter!(
        publish_ceremony_definition,
        PublishCeremonyDefinitionUseCase,
        publish_ceremony_definition
    );
    setter!(auto_dispatch, AutoDispatchService, auto_dispatch);

    #[must_use]
    pub fn statistics(mut self, value: Arc<dyn StatisticsPort>) -> Self {
        self.statistics = Some(value);
        self
    }

    #[must_use]
    pub fn ceremony_definitions(
        mut self,
        value: Arc<dyn CeremonyDefinitionRepositoryPort>,
    ) -> Self {
        self.ceremony_definitions = Some(value);
        self
    }

    #[must_use]
    pub fn contract_registry(mut self, value: Arc<dyn ContractRegistryPort>) -> Self {
        self.contract_registry = Some(value);
        self
    }

    #[must_use]
    pub fn service_version(mut self, value: &'static str) -> Self {
        self.service_version = Some(value);
        self
    }

    /// Consume the builder. Missing dependencies are reported via
    /// [`DomainError::InvariantViolated`] so wiring errors surface
    /// through the same error channel the rest of the app uses.
    pub fn build(self) -> Result<ChoreographerGrpcService, DomainError> {
        Ok(ChoreographerGrpcService {
            deliberate: required!(self, deliberate),
            orchestrate: required!(self, orchestrate),
            create_council: required!(self, create_council),
            delete_council: required!(self, delete_council),
            list_councils: required!(self, list_councils),
            get_deliberation: required!(self, get_deliberation),
            register_agent: required!(self, register_agent),
            unregister_agent: required!(self, unregister_agent),
            run_council_decision: required!(self, run_council_decision),
            run_ceremony: required!(self, run_ceremony),
            get_ceremony_instance: required!(self, get_ceremony_instance),
            list_ceremony_instances: required!(self, list_ceremony_instances),
            resolve_ceremony_definition: required!(self, resolve_ceremony_definition),
            start_ceremony: required!(self, start_ceremony),
            start_published_ceremony: required!(self, start_published_ceremony),
            run_ceremony_step: required!(self, run_ceremony_step),
            apply_ceremony_transition: required!(self, apply_ceremony_transition),
            approve_ceremony_guard: required!(self, approve_ceremony_guard),
            defer_ceremony_guard: required!(self, defer_ceremony_guard),
            request_ceremony_intervention: required!(self, request_ceremony_intervention),
            respond_to_ceremony_intervention: required!(self, respond_to_ceremony_intervention),
            close_ceremony_intervention: required!(self, close_ceremony_intervention),
            collect_ceremony_evidence: required!(self, collect_ceremony_evidence),
            publish_ceremony_definition: required!(self, publish_ceremony_definition),
            diff_ceremony_definitions: required!(self, diff_ceremony_definitions),
            bind_ceremony_participants: required!(self, bind_ceremony_participants),
            ceremony_definitions: required!(self, ceremony_definitions, "port"),
            prepare_ceremony_participants: required!(self, prepare_ceremony_participants),
            contract_registry: required!(self, contract_registry, "port"),
            auto_dispatch: required!(self, auto_dispatch, "service"),
            statistics: required!(self, statistics, "port"),
            started_at: std::time::Instant::now(),
            service_version: self.service_version.unwrap_or(""),
        })
    }
}

type GrpcResult<T> = std::result::Result<Response<T>, Status>;

#[async_trait]
impl ChoreographerService for ChoreographerGrpcService {
    type StreamDeliberationStream = tokio_stream::wrappers::ReceiverStream<
        std::result::Result<pb::StreamDeliberationResponse, Status>,
    >;

    #[tracing::instrument(name = "rpc.deliberate", skip_all)]
    async fn deliberate(
        &self,
        request: Request<pb::DeliberateRequest>,
    ) -> GrpcResult<pb::DeliberateResponse> {
        link_span_to_metadata(&request);
        let task_proto = request
            .into_inner()
            .task
            .ok_or_else(|| Status::invalid_argument("task is required"))?;
        let task = task_from_proto(task_proto).map_err(domain_error_to_status)?;
        let out = self
            .deliberate
            .execute(task)
            .await
            .map_err(domain_error_to_status)?;
        debug!(
            task_id = out.deliberation.task_id().as_str(),
            winner = out.winner_proposal_id.as_str(),
            "deliberate rpc ok"
        );
        Ok(Response::new(deliberate_response_from(&out)))
    }

    #[tracing::instrument(name = "rpc.stream_deliberation", skip_all)]
    async fn stream_deliberation(
        &self,
        request: Request<pb::StreamDeliberationRequest>,
    ) -> GrpcResult<Self::StreamDeliberationStream> {
        link_span_to_metadata(&request);
        let task_proto = request
            .into_inner()
            .task
            .ok_or_else(|| Status::invalid_argument("task is required"))?;
        let task = task_from_proto(task_proto).map_err(domain_error_to_status)?;

        // Bounded channel: backpressure shields the deliberation from
        // unbounded buffering if the client reads slowly, and the
        // observer is a no-op on sink-closed so a slow/disconnected
        // client never deadlocks the use case.
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let observer: Arc<dyn choreo_core::ports::DeliberationObserverPort> =
            Arc::new(super::stream::ChannelObserver::new(tx.clone()));
        let usecase = self.deliberate.clone();

        tokio::spawn(async move {
            match usecase.execute_with_observer(task, observer).await {
                Ok(out) => {
                    // Final frame carries the winner projection so
                    // clients that only wanted the result can read
                    // exactly one message and close.
                    let response = deliberate_response_from(&out);
                    let winner_result = response.results.first().cloned().unwrap_or_default();
                    let frame = pb::StreamDeliberationResponse {
                        update: Some(pb::DeliberationUpdate {
                            task_id: response.task_id,
                            phase: pb::DeliberationPhase::Completed as i32,
                            emitted_at: None,
                            payload: Some(pb::deliberation_update::Payload::Result(winner_result)),
                        }),
                    };
                    let _ = tx.send(Ok(frame)).await;
                }
                Err(err) => {
                    let _ = tx.send(Err(domain_error_to_status(err))).await;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    #[tracing::instrument(name = "rpc.get_deliberation_result", skip_all)]
    async fn get_deliberation_result(
        &self,
        request: Request<pb::GetDeliberationResultRequest>,
    ) -> GrpcResult<pb::GetDeliberationResultResponse> {
        link_span_to_metadata(&request);
        let task_id = TaskId::new(request.into_inner().task_id).map_err(domain_error_to_status)?;
        match self.get_deliberation.execute(&task_id).await {
            Ok(deliberation) => {
                let winner =
                    deliberation.ranking().first().cloned().unwrap_or_else(|| {
                        choreo_core::value_objects::ProposalId::new("_").unwrap()
                    });
                let out = choreo_app::usecases::DeliberateOutput {
                    deliberation,
                    winner_proposal_id: winner,
                };
                Ok(Response::new(pb::GetDeliberationResultResponse {
                    found: true,
                    result: Some(deliberate_response_from(&out)),
                }))
            }
            Err(DomainError::NotFound { .. }) => {
                Ok(Response::new(pb::GetDeliberationResultResponse {
                    found: false,
                    result: None,
                }))
            }
            Err(err) => Err(domain_error_to_status(err)),
        }
    }

    #[tracing::instrument(name = "rpc.orchestrate", skip_all)]
    async fn orchestrate(
        &self,
        request: Request<pb::OrchestrateRequest>,
    ) -> GrpcResult<pb::OrchestrateResponse> {
        link_span_to_metadata(&request);
        let req = request.into_inner();
        let task_proto = req
            .task
            .ok_or_else(|| Status::invalid_argument("task is required"))?;
        let task = task_from_proto(task_proto).map_err(domain_error_to_status)?;
        // execution_options flows untouched to the executor adapter.
        let options = super::mappers::attributes_from_struct(req.execution_options)
            .map_err(domain_error_to_status)?;
        let out = self
            .orchestrate
            .execute(task, options)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(orchestrate_response_from(&out)))
    }

    #[tracing::instrument(name = "rpc.create_council", skip_all)]
    async fn create_council(
        &self,
        request: Request<pb::CreateCouncilRequest>,
    ) -> GrpcResult<pb::CreateCouncilResponse> {
        link_span_to_metadata(&request);
        let req = request.into_inner();
        // Bound the council size through the domain value object: rejects
        // zero and caps at MAX_NUM_AGENTS, so a hostile `num_agents` can't
        // request a multi-billion-id allocation.
        let num_agents = choreo_core::value_objects::NumAgents::new(req.num_agents)
            .map_err(domain_error_to_status)?;
        let n = num_agents.get() as usize;
        // The create-council RPC does not carry pre-minted agent ids;
        // we mint one id per slot and expect the caller to have
        // previously registered matching agents through the (future)
        // RegisterAgent RPC or through the composition root.
        let agent_ids: Vec<AgentId> = (0..n)
            .map(|i| AgentId::new(format!("agent-{}-{}", req.specialty, i)))
            .collect::<Result<_, _>>()
            .map_err(domain_error_to_status)?;

        let council_id =
            choreo_core::value_objects::CouncilId::new(uuid::Uuid::new_v4().to_string())
                .map_err(domain_error_to_status)?;
        let specialty = Specialty::new(&req.specialty).map_err(domain_error_to_status)?;

        let council = self
            .create_council
            .execute(CreateCouncilInput {
                council_id,
                specialty,
                agents: agent_ids,
            })
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(pb::CreateCouncilResponse {
            council: Some(council_summary_from(&council, vec![])),
        }))
    }

    #[tracing::instrument(name = "rpc.list_councils", skip_all)]
    async fn list_councils(
        &self,
        request: Request<pb::ListCouncilsRequest>,
    ) -> GrpcResult<pb::ListCouncilsResponse> {
        link_span_to_metadata(&request);
        let _ = request;
        let councils = self
            .list_councils
            .execute()
            .await
            .map_err(domain_error_to_status)?;
        let summaries = councils
            .iter()
            .map(|c| council_summary_from(c, vec![]))
            .collect();
        Ok(Response::new(pb::ListCouncilsResponse {
            councils: summaries,
        }))
    }

    #[tracing::instrument(name = "rpc.delete_council", skip_all)]
    async fn delete_council(
        &self,
        request: Request<pb::DeleteCouncilRequest>,
    ) -> GrpcResult<pb::DeleteCouncilResponse> {
        link_span_to_metadata(&request);
        let specialty =
            Specialty::new(request.into_inner().specialty).map_err(domain_error_to_status)?;
        match self.delete_council.execute(&specialty).await {
            Ok(()) => Ok(Response::new(pb::DeleteCouncilResponse { deleted: true })),
            Err(DomainError::NotFound { .. }) => {
                Ok(Response::new(pb::DeleteCouncilResponse { deleted: false }))
            }
            Err(err) => Err(domain_error_to_status(err)),
        }
    }

    #[tracing::instrument(name = "rpc.register_agent", skip_all)]
    async fn register_agent(
        &self,
        request: Request<pb::RegisterAgentRequest>,
    ) -> GrpcResult<pb::RegisterAgentResponse> {
        link_span_to_metadata(&request);
        let descriptor =
            descriptor_from_register_request(request.into_inner()).map_err(|err| match err {
                DescriptorError::MissingAgentSummary => {
                    Status::invalid_argument("agent summary is required")
                }
                DescriptorError::Domain(err) => domain_error_to_status(err),
            })?;
        let id = self
            .register_agent
            .execute(descriptor)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::RegisterAgentResponse {
            agent_id: id.into_inner(),
        }))
    }

    #[tracing::instrument(name = "rpc.unregister_agent", skip_all)]
    async fn unregister_agent(
        &self,
        request: Request<pb::UnregisterAgentRequest>,
    ) -> GrpcResult<pb::UnregisterAgentResponse> {
        link_span_to_metadata(&request);
        let id = AgentId::new(request.into_inner().agent_id).map_err(domain_error_to_status)?;
        match self.unregister_agent.execute(&id).await {
            Ok(()) => Ok(Response::new(pb::UnregisterAgentResponse {
                unregistered: true,
            })),
            Err(DomainError::NotFound { .. }) => Ok(Response::new(pb::UnregisterAgentResponse {
                unregistered: false,
            })),
            Err(err) => Err(domain_error_to_status(err)),
        }
    }

    #[tracing::instrument(name = "rpc.run_council_decision", skip_all)]
    async fn run_council_decision(
        &self,
        request: Request<pb::RunCouncilDecisionRequest>,
    ) -> GrpcResult<pb::RunCouncilDecisionResponse> {
        link_span_to_metadata(&request);
        let input =
            run_council_decision_input_from_proto(request.into_inner()).map_err(Status::from)?;
        let output = self
            .run_council_decision
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        debug!(
            task_id = output.task_id.as_str(),
            passed = output.passed,
            duration_ms = output.duration_ms.get(),
            "run_council_decision rpc ok"
        );
        Ok(Response::new(run_council_decision_response_from(&output)))
    }

    #[tracing::instrument(name = "rpc.register_contract", skip_all)]
    async fn register_contract(
        &self,
        request: Request<pb::RegisterContractRequest>,
    ) -> GrpcResult<pb::RegisterContractResponse> {
        link_span_to_metadata(&request);
        let proto = request
            .into_inner()
            .contract
            .ok_or_else(|| Status::invalid_argument("contract is required"))?;
        let contract = output_contract_from_proto(Some(proto))
            .map_err(domain_error_to_status)?
            .ok_or_else(|| Status::invalid_argument("contract is required"))?;
        let contract_id = contract.contract_id().to_owned();
        self.contract_registry
            .register(contract)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::RegisterContractResponse { contract_id }))
    }

    #[tracing::instrument(name = "rpc.list_contracts", skip_all)]
    async fn list_contracts(
        &self,
        request: Request<pb::ListContractsRequest>,
    ) -> GrpcResult<pb::ListContractsResponse> {
        link_span_to_metadata(&request);
        let _ = request;
        let contracts = self
            .contract_registry
            .list()
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::ListContractsResponse {
            contracts: contracts.iter().map(output_contract_to_proto).collect(),
        }))
    }

    #[tracing::instrument(name = "rpc.delete_contract", skip_all)]
    async fn delete_contract(
        &self,
        request: Request<pb::DeleteContractRequest>,
    ) -> GrpcResult<pb::DeleteContractResponse> {
        link_span_to_metadata(&request);
        let contract_id = request.into_inner().contract_id;
        match self.contract_registry.delete(&contract_id).await {
            Ok(()) => Ok(Response::new(pb::DeleteContractResponse { deleted: true })),
            Err(DomainError::NotFound { .. }) => {
                Ok(Response::new(pb::DeleteContractResponse { deleted: false }))
            }
            Err(err) => Err(domain_error_to_status(err)),
        }
    }

    #[tracing::instrument(name = "rpc.process_trigger_event", skip_all)]
    async fn process_trigger_event(
        &self,
        request: Request<pb::ProcessTriggerEventRequest>,
    ) -> GrpcResult<pb::ProcessTriggerEventResponse> {
        link_span_to_metadata(&request);
        let inner = request.into_inner();
        let ev_proto = inner
            .event
            .ok_or_else(|| Status::invalid_argument("event is required"))?;
        let trigger = trigger_event_from_proto(ev_proto, time::OffsetDateTime::now_utc())
            .map_err(domain_error_to_status)?;

        let outcome = self
            .auto_dispatch
            .dispatch(&trigger)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(pb::ProcessTriggerEventResponse {
            ack: Some(pb::TriggerAck {
                event_id: trigger.envelope().event_id().as_str().to_owned(),
                accepted: outcome.accepted(),
                dispatched_task_ids: outcome
                    .dispatched_task_ids()
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect(),
                reason: if outcome.accepted() {
                    String::new()
                } else {
                    "no specialties produced a deliberation".to_owned()
                },
            }),
        }))
    }

    #[tracing::instrument(name = "rpc.run_ceremony", skip_all)]
    async fn run_ceremony(
        &self,
        request: Request<pb::RunCeremonyRequest>,
    ) -> GrpcResult<pb::RunCeremonyResponse> {
        link_span_to_metadata(&request);
        let input =
            run_ceremony_input_from_proto(request.into_inner()).map_err(domain_error_to_status)?;
        let participant_input = CeremonyParticipantPlanAdapter::from_definition(input.definition())
            .map_err(domain_error_to_status)?;
        self.prepare_ceremony_participants
            .execute(participant_input)
            .await
            .map_err(domain_error_to_status)?;
        let output = self
            .run_ceremony
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        debug!(
            ceremony_id = output.instance().id().as_str(),
            final_state = output.instance().current_state().as_str(),
            completed = output.instance().is_completed(output.definition()),
            "run_ceremony rpc ok"
        );
        Ok(Response::new(run_ceremony_response_from(&output)))
    }

    #[tracing::instrument(name = "rpc.start_ceremony", skip_all)]
    async fn start_ceremony(
        &self,
        request: Request<pb::StartCeremonyRequest>,
    ) -> GrpcResult<pb::StartCeremonyResponse> {
        link_span_to_metadata(&request);
        let StartCeremonyFromYaml { definition, input } =
            start_ceremony_from_proto(request.into_inner()).map_err(domain_error_to_status)?;
        // A session started from supplied YAML has to be able to find
        // its definition again on the next call, which may well land
        // on a different process.
        self.ceremony_definitions
            .save(&definition)
            .await
            .map_err(domain_error_to_status)?;
        self.prepare_participants(&definition).await?;
        let instance = self
            .start_ceremony
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::StartCeremonyResponse {
            instance: Some(Self::render(&instance, &definition).map_err(domain_error_to_status)?),
        }))
    }

    #[tracing::instrument(name = "rpc.start_published_ceremony", skip_all)]
    async fn start_published_ceremony(
        &self,
        request: Request<pb::StartPublishedCeremonyRequest>,
    ) -> GrpcResult<pb::StartPublishedCeremonyResponse> {
        link_span_to_metadata(&request);
        let input = start_published_ceremony_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .start_published_ceremony
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        // Resolved rather than read from the publication directly: the
        // instance records a digest, and resolving it back through the
        // same path every other RPC uses is what proves the digest it
        // recorded still matches what is published.
        let definition = self
            .resolve_ceremony_definition
            .execute(&instance)
            .await
            .map_err(domain_error_to_status)?;
        self.prepare_participants(&definition).await?;
        Ok(Response::new(pb::StartPublishedCeremonyResponse {
            instance: Some(Self::render(&instance, &definition).map_err(domain_error_to_status)?),
        }))
    }

    #[tracing::instrument(name = "rpc.run_ceremony_step", skip_all)]
    async fn run_ceremony_step(
        &self,
        request: Request<pb::RunCeremonyStepRequest>,
    ) -> GrpcResult<pb::RunCeremonyStepResponse> {
        link_span_to_metadata(&request);
        let request = request.into_inner();
        let ceremony_id =
            CeremonyId::new(request.ceremony_id.clone()).map_err(domain_error_to_status)?;
        let (instance, definition) = self.session(&ceremony_id).await?;
        let input = run_ceremony_step_input_from_proto(request, &definition, &instance)
            .map_err(domain_error_to_status)?;
        let output = self
            .run_ceremony_step
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::RunCeremonyStepResponse {
            instance: Some(
                Self::render(output.instance(), &definition).map_err(domain_error_to_status)?,
            ),
        }))
    }

    #[tracing::instrument(name = "rpc.apply_ceremony_transition", skip_all)]
    async fn apply_ceremony_transition(
        &self,
        request: Request<pb::ApplyCeremonyTransitionRequest>,
    ) -> GrpcResult<pb::ApplyCeremonyTransitionResponse> {
        link_span_to_metadata(&request);
        let request = request.into_inner();
        let ceremony_id =
            CeremonyId::new(request.ceremony_id.clone()).map_err(domain_error_to_status)?;
        let (instance, definition) = self.session(&ceremony_id).await?;
        let input = apply_ceremony_transition_input_from_proto(request, &definition, &instance)
            .map_err(domain_error_to_status)?;
        let moved = self
            .apply_ceremony_transition
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::ApplyCeremonyTransitionResponse {
            instance: Some(Self::render(&moved, &definition).map_err(domain_error_to_status)?),
        }))
    }

    // Each of these answers with the session, like every other move.
    // The definition is resolved once per call rather than carried
    // over from a previous one: these are the calls a person makes,
    // minutes or hours apart, and nothing says the process handling
    // this one saw the last.

    #[tracing::instrument(name = "rpc.approve_ceremony_guard", skip_all)]
    async fn approve_ceremony_guard(
        &self,
        request: Request<pb::ApproveCeremonyGuardRequest>,
    ) -> GrpcResult<pb::ApproveCeremonyGuardResponse> {
        link_span_to_metadata(&request);
        let input = approve_ceremony_guard_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .approve_ceremony_guard
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::ApproveCeremonyGuardResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.defer_ceremony_guard", skip_all)]
    async fn defer_ceremony_guard(
        &self,
        request: Request<pb::DeferCeremonyGuardRequest>,
    ) -> GrpcResult<pb::DeferCeremonyGuardResponse> {
        link_span_to_metadata(&request);
        let input = defer_ceremony_guard_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .defer_ceremony_guard
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::DeferCeremonyGuardResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.request_ceremony_intervention", skip_all)]
    async fn request_ceremony_intervention(
        &self,
        request: Request<pb::RequestCeremonyInterventionRequest>,
    ) -> GrpcResult<pb::RequestCeremonyInterventionResponse> {
        link_span_to_metadata(&request);
        let input = request_ceremony_intervention_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .request_ceremony_intervention
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::RequestCeremonyInterventionResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.respond_to_ceremony_intervention", skip_all)]
    async fn respond_to_ceremony_intervention(
        &self,
        request: Request<pb::RespondToCeremonyInterventionRequest>,
    ) -> GrpcResult<pb::RespondToCeremonyInterventionResponse> {
        link_span_to_metadata(&request);
        let input = respond_to_ceremony_intervention_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .respond_to_ceremony_intervention
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::RespondToCeremonyInterventionResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.close_ceremony_intervention", skip_all)]
    async fn close_ceremony_intervention(
        &self,
        request: Request<pb::CloseCeremonyInterventionRequest>,
    ) -> GrpcResult<pb::CloseCeremonyInterventionResponse> {
        link_span_to_metadata(&request);
        let input = close_ceremony_intervention_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .close_ceremony_intervention
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::CloseCeremonyInterventionResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.collect_ceremony_evidence", skip_all)]
    async fn collect_ceremony_evidence(
        &self,
        request: Request<pb::CollectCeremonyEvidenceRequest>,
    ) -> GrpcResult<pb::CollectCeremonyEvidenceResponse> {
        link_span_to_metadata(&request);
        let input = collect_ceremony_evidence_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .collect_ceremony_evidence
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::CollectCeremonyEvidenceResponse {
            instance: Some(state),
        }))
    }

    // Validating and explaining touch nothing: they answer about the
    // YAML in the request. A draft is not a definition until someone
    // publishes it, and that distinction is the point of having three
    // calls instead of one.
    #[tracing::instrument(name = "rpc.validate_ceremony_draft", skip_all)]
    async fn validate_ceremony_draft(
        &self,
        request: Request<pb::ValidateCeremonyDraftRequest>,
    ) -> GrpcResult<pb::ValidateCeremonyDraftResponse> {
        link_span_to_metadata(&request);
        let draft = CeremonyDefinitionYaml::parse_draft_str(&request.into_inner().definition_yaml)
            .map_err(domain_error_to_status)?;
        let report = draft.analyze();
        Ok(Response::new(validate_ceremony_draft_response_from(
            &CeremonyDraftView::project(&draft, &report),
        )))
    }

    #[tracing::instrument(name = "rpc.explain_ceremony_draft", skip_all)]
    async fn explain_ceremony_draft(
        &self,
        request: Request<pb::ExplainCeremonyDraftRequest>,
    ) -> GrpcResult<pb::ExplainCeremonyDraftResponse> {
        link_span_to_metadata(&request);
        let draft = CeremonyDefinitionYaml::parse_draft_str(&request.into_inner().definition_yaml)
            .map_err(domain_error_to_status)?;
        let report = draft.analyze();
        Ok(Response::new(explain_ceremony_draft_response_from(
            &CeremonyDraftView::project(&draft, &report),
        )))
    }

    #[tracing::instrument(name = "rpc.publish_ceremony_definition", skip_all)]
    async fn publish_ceremony_definition(
        &self,
        request: Request<pb::PublishCeremonyDefinitionRequest>,
    ) -> GrpcResult<pb::PublishCeremonyDefinitionResponse> {
        link_span_to_metadata(&request);
        let definition = CeremonyDefinitionYaml::parse_str(&request.into_inner().definition_yaml)
            .map_err(domain_error_to_status)?;
        let outcome = self
            .publish_ceremony_definition
            .execute(definition)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(publish_ceremony_definition_response_from(
            &outcome,
        )))
    }

    #[tracing::instrument(name = "rpc.bind_ceremony_participants", skip_all)]
    async fn bind_ceremony_participants(
        &self,
        request: Request<pb::BindCeremonyParticipantsRequest>,
    ) -> GrpcResult<pb::BindCeremonyParticipantsResponse> {
        link_span_to_metadata(&request);
        let input = bind_ceremony_participants_input_from_proto(request.into_inner())
            .map_err(domain_error_to_status)?;
        let instance = self
            .bind_ceremony_participants
            .execute(input)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::BindCeremonyParticipantsResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.diff_ceremony_definitions", skip_all)]
    async fn diff_ceremony_definitions(
        &self,
        request: Request<pb::DiffCeremonyDefinitionsRequest>,
    ) -> GrpcResult<pb::DiffCeremonyDefinitionsResponse> {
        link_span_to_metadata(&request);
        let request = request.into_inner();
        let before = ceremony_definition_source_from_proto(request.before, "before")
            .map_err(domain_error_to_status)?;
        let after = ceremony_definition_source_from_proto(request.after, "after")
            .map_err(domain_error_to_status)?;
        let diff = self
            .diff_ceremony_definitions
            .execute(before, after)
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(diff_ceremony_definitions_response_from(
            &diff,
        )))
    }

    #[tracing::instrument(name = "rpc.get_ceremony_instance", skip_all)]
    async fn get_ceremony_instance(
        &self,
        request: Request<pb::GetCeremonyInstanceRequest>,
    ) -> GrpcResult<pb::GetCeremonyInstanceResponse> {
        link_span_to_metadata(&request);
        let ceremony_id =
            CeremonyId::new(request.into_inner().ceremony_id).map_err(domain_error_to_status)?;
        let instance = self
            .get_ceremony_instance
            .execute(&ceremony_id)
            .await
            .map_err(domain_error_to_status)?;
        let state = self.project(&instance).await?;
        Ok(Response::new(pb::GetCeremonyInstanceResponse {
            instance: Some(state),
        }))
    }

    #[tracing::instrument(name = "rpc.list_ceremony_instances", skip_all)]
    async fn list_ceremony_instances(
        &self,
        request: Request<pb::ListCeremonyInstancesRequest>,
    ) -> GrpcResult<pb::ListCeremonyInstancesResponse> {
        link_span_to_metadata(&request);
        let instances = self
            .list_ceremony_instances
            .execute()
            .await
            .map_err(domain_error_to_status)?;
        let mut states = Vec::with_capacity(instances.len());
        for instance in &instances {
            states.push(self.project(instance).await?);
        }
        Ok(Response::new(pb::ListCeremonyInstancesResponse {
            instances: states,
        }))
    }

    #[tracing::instrument(name = "rpc.get_status", skip_all)]
    async fn get_status(
        &self,
        request: Request<pb::GetStatusRequest>,
    ) -> GrpcResult<pb::GetStatusResponse> {
        link_span_to_metadata(&request);
        let include_stats = request.into_inner().include_stats;
        let stats = if include_stats {
            Some(
                self.statistics
                    .snapshot()
                    .await
                    .map_err(domain_error_to_status)?,
            )
        } else {
            None
        };

        Ok(Response::new(pb::GetStatusResponse {
            version: self.service_version.to_owned(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            health: "healthy".to_owned(),
            stats: stats.as_ref().map(statistics_to_proto),
        }))
    }

    #[tracing::instrument(name = "rpc.get_metrics", skip_all)]
    async fn get_metrics(
        &self,
        request: Request<pb::GetMetricsRequest>,
    ) -> GrpcResult<pb::GetMetricsResponse> {
        link_span_to_metadata(&request);
        let _ = request;
        let snap = self
            .statistics
            .snapshot()
            .await
            .map_err(domain_error_to_status)?;
        Ok(Response::new(pb::GetMetricsResponse {
            stats: Some(statistics_to_proto(&snap)),
        }))
    }
}

/// Errors surfaced while turning a [`pb::RegisterAgentRequest`] into a
/// domain [`AgentDescriptor`].
#[derive(Debug)]
enum DescriptorError {
    MissingAgentSummary,
    Domain(DomainError),
}

impl From<DomainError> for DescriptorError {
    fn from(err: DomainError) -> Self {
        Self::Domain(err)
    }
}

/// Map the proto request into a domain descriptor.
///
/// Precedence on specialty: the dedicated top-level `specialty` field
/// on the request wins when non-empty; otherwise the nested
/// `agent.specialty` is used. This keeps the proto backwards-
/// compatible without encoding two sources of truth downstream.
fn descriptor_from_register_request(
    req: pb::RegisterAgentRequest,
) -> Result<AgentDescriptor, DescriptorError> {
    let summary = req.agent.ok_or(DescriptorError::MissingAgentSummary)?;
    let specialty_str = if req.specialty.trim().is_empty() {
        summary.specialty
    } else {
        req.specialty
    };
    Ok(AgentDescriptor {
        id: AgentId::new(summary.agent_id)?,
        specialty: Specialty::new(specialty_str)?,
        kind: AgentKind::new(summary.kind)?,
        attributes: super::mappers::attributes_from_struct(req.agent_config)?,
    })
}

/// Map the domain [`choreo_core::entities::Statistics`] into the
/// protobuf `Statistics` message. Kept here, next to the only call
/// sites, because it is a pure transport concern.
fn statistics_to_proto(stats: &choreo_core::entities::Statistics) -> pb::Statistics {
    let per_specialty_counts = stats
        .per_specialty()
        .iter()
        .map(|(sp, count)| (sp.as_str().to_owned(), *count))
        .collect();
    pb::Statistics {
        total_deliberations: stats.total_deliberations(),
        total_orchestrations: stats.total_orchestrations(),
        total_duration_ms: stats.total_duration().get(),
        average_duration_ms: stats.average_duration_ms(),
        per_specialty_counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::entities::Statistics;
    use choreo_core::value_objects::{DurationMs, Specialty};

    #[test]
    fn statistics_to_proto_maps_every_field() {
        let mut stats = Statistics::new();
        stats.record_deliberation(
            &Specialty::new("triage").unwrap(),
            DurationMs::from_millis(100),
        );
        stats.record_deliberation(
            &Specialty::new("triage").unwrap(),
            DurationMs::from_millis(50),
        );
        stats.record_deliberation(
            &Specialty::new("reviewer").unwrap(),
            DurationMs::from_millis(200),
        );
        stats.record_orchestration(DurationMs::from_millis(400));

        let mapped = statistics_to_proto(&stats);
        assert_eq!(mapped.total_deliberations, 3);
        assert_eq!(mapped.total_orchestrations, 1);
        assert_eq!(mapped.total_duration_ms, 750);
        // (100 + 50 + 200 + 400) / 4 ops = 187.5
        assert!((mapped.average_duration_ms - 187.5).abs() < 1e-9);
        assert_eq!(mapped.per_specialty_counts.get("triage").copied(), Some(2));
        assert_eq!(
            mapped.per_specialty_counts.get("reviewer").copied(),
            Some(1)
        );
    }

    #[test]
    fn statistics_to_proto_empty_maps_zeros_and_empty_map() {
        let stats = Statistics::default();
        let mapped = statistics_to_proto(&stats);
        assert_eq!(mapped.total_deliberations, 0);
        assert_eq!(mapped.total_orchestrations, 0);
        assert_eq!(mapped.total_duration_ms, 0);
        assert!((mapped.average_duration_ms - 0.0).abs() < f64::EPSILON);
        assert!(mapped.per_specialty_counts.is_empty());
    }

    fn summary(id: &str, specialty: &str, kind: &str) -> pb::AgentSummary {
        pb::AgentSummary {
            agent_id: id.to_owned(),
            specialty: specialty.to_owned(),
            kind: kind.to_owned(),
            attributes: None,
        }
    }

    #[test]
    fn descriptor_from_request_uses_top_level_specialty_when_present() {
        let req = pb::RegisterAgentRequest {
            specialty: "reviewer".to_owned(),
            agent: Some(summary("a1", "triage", "noop")),
            agent_config: None,
        };
        let d = descriptor_from_register_request(req).unwrap();
        assert_eq!(d.id.as_str(), "a1");
        assert_eq!(d.specialty.as_str(), "reviewer");
        assert_eq!(d.kind.as_str(), "noop");
        assert!(d.attributes.is_empty());
    }

    #[test]
    fn descriptor_from_request_falls_back_to_nested_specialty_when_empty() {
        let req = pb::RegisterAgentRequest {
            specialty: "   ".to_owned(),
            agent: Some(summary("a1", "triage", "noop")),
            agent_config: None,
        };
        let d = descriptor_from_register_request(req).unwrap();
        assert_eq!(d.specialty.as_str(), "triage");
    }

    #[test]
    fn descriptor_from_request_missing_agent_is_reported() {
        let req = pb::RegisterAgentRequest {
            specialty: "triage".to_owned(),
            agent: None,
            agent_config: None,
        };
        let err = descriptor_from_register_request(req).unwrap_err();
        assert!(matches!(err, DescriptorError::MissingAgentSummary));
    }

    #[test]
    fn descriptor_from_request_domain_validation_propagates() {
        // Empty kind fails at AgentKind construction.
        let req = pb::RegisterAgentRequest {
            specialty: "triage".to_owned(),
            agent: Some(summary("a1", "triage", "")),
            agent_config: None,
        };
        let err = descriptor_from_register_request(req).unwrap_err();
        assert!(matches!(err, DescriptorError::Domain(_)));
    }
}
