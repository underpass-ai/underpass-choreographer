//! gRPC service handler — thin translation from proto RPCs onto
//! use cases in [`choreo_app`].

use std::sync::Arc;

use async_trait::async_trait;
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::{
    CreateCouncilInput, CreateCouncilUseCase, DeleteCouncilUseCase, DeliberateUseCase,
    GetDeliberationUseCase, ListCouncilsUseCase, OrchestrateUseCase,
};
use choreo_core::error::DomainError;
use choreo_core::ports::StatisticsPort;
use choreo_core::value_objects::{AgentId, Specialty, TaskId};
use choreo_proto::v1 as pb;
use choreo_proto::v1::choreographer_service_server::{
    ChoreographerService, ChoreographerServiceServer,
};
use tonic::{Request, Response, Status};
use tracing::debug;

use super::mappers::{
    council_summary_from, deliberate_response_from, orchestrate_response_from, task_from_proto,
    trigger_event_from_proto,
};
use super::status::domain_error_to_status;

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
    auto_dispatch: Option<Arc<AutoDispatchService>>,
    statistics: Option<Arc<dyn StatisticsPort>>,
    service_version: Option<&'static str>,
}

impl std::fmt::Debug for ChoreographerGrpcServiceBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChoreographerGrpcServiceBuilder").finish()
    }
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
    setter!(auto_dispatch, AutoDispatchService, auto_dispatch);

    #[must_use]
    pub fn statistics(mut self, value: Arc<dyn StatisticsPort>) -> Self {
        self.statistics = Some(value);
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
            deliberate: self.deliberate.ok_or(DomainError::InvariantViolated {
                reason: "grpc: deliberate use case is required",
            })?,
            orchestrate: self.orchestrate.ok_or(DomainError::InvariantViolated {
                reason: "grpc: orchestrate use case is required",
            })?,
            create_council: self.create_council.ok_or(DomainError::InvariantViolated {
                reason: "grpc: create_council use case is required",
            })?,
            delete_council: self.delete_council.ok_or(DomainError::InvariantViolated {
                reason: "grpc: delete_council use case is required",
            })?,
            list_councils: self.list_councils.ok_or(DomainError::InvariantViolated {
                reason: "grpc: list_councils use case is required",
            })?,
            get_deliberation: self
                .get_deliberation
                .ok_or(DomainError::InvariantViolated {
                    reason: "grpc: get_deliberation use case is required",
                })?,
            auto_dispatch: self.auto_dispatch.ok_or(DomainError::InvariantViolated {
                reason: "grpc: auto_dispatch service is required",
            })?,
            statistics: self.statistics.ok_or(DomainError::InvariantViolated {
                reason: "grpc: statistics port is required",
            })?,
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

    async fn deliberate(
        &self,
        request: Request<pb::DeliberateRequest>,
    ) -> GrpcResult<pb::DeliberateResponse> {
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

    async fn stream_deliberation(
        &self,
        _request: Request<pb::StreamDeliberationRequest>,
    ) -> GrpcResult<Self::StreamDeliberationStream> {
        Err(Status::unimplemented(
            "StreamDeliberation is not implemented yet; see service docblock",
        ))
    }

    async fn get_deliberation_result(
        &self,
        request: Request<pb::GetDeliberationResultRequest>,
    ) -> GrpcResult<pb::GetDeliberationResultResponse> {
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

    async fn orchestrate(
        &self,
        request: Request<pb::OrchestrateRequest>,
    ) -> GrpcResult<pb::OrchestrateResponse> {
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

    async fn create_council(
        &self,
        request: Request<pb::CreateCouncilRequest>,
    ) -> GrpcResult<pb::CreateCouncilResponse> {
        let req = request.into_inner();
        let n = usize::try_from(req.num_agents).unwrap_or(0);
        if n == 0 {
            return Err(Status::invalid_argument("num_agents must be > 0"));
        }
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

    async fn list_councils(
        &self,
        _request: Request<pb::ListCouncilsRequest>,
    ) -> GrpcResult<pb::ListCouncilsResponse> {
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

    async fn delete_council(
        &self,
        request: Request<pb::DeleteCouncilRequest>,
    ) -> GrpcResult<pb::DeleteCouncilResponse> {
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

    async fn register_agent(
        &self,
        _request: Request<pb::RegisterAgentRequest>,
    ) -> GrpcResult<pb::RegisterAgentResponse> {
        Err(Status::unimplemented(
            "RegisterAgent is not implemented yet; see service docblock",
        ))
    }

    async fn unregister_agent(
        &self,
        _request: Request<pb::UnregisterAgentRequest>,
    ) -> GrpcResult<pb::UnregisterAgentResponse> {
        Err(Status::unimplemented(
            "UnregisterAgent is not implemented yet; see service docblock",
        ))
    }

    async fn process_trigger_event(
        &self,
        request: Request<pb::ProcessTriggerEventRequest>,
    ) -> GrpcResult<pb::ProcessTriggerEventResponse> {
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

    async fn get_status(
        &self,
        request: Request<pb::GetStatusRequest>,
    ) -> GrpcResult<pb::GetStatusResponse> {
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

    async fn get_metrics(
        &self,
        _request: Request<pb::GetMetricsRequest>,
    ) -> GrpcResult<pb::GetMetricsResponse> {
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
        stats.record_deliberation(&Specialty::new("triage").unwrap(), DurationMs::from_millis(100));
        stats.record_deliberation(&Specialty::new("triage").unwrap(), DurationMs::from_millis(50));
        stats.record_deliberation(&Specialty::new("reviewer").unwrap(), DurationMs::from_millis(200));
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
}
