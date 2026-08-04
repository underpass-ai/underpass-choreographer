//! [`EmbeddedChoreographer`] as an implementation of the published contract.
//!
//! The conversion in this module is the whole of the coupling a consumer is
//! allowed: domain aggregate in, plain view out. Nothing of `choreo-core`
//! crosses the trait.

use std::collections::BTreeMap;

use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_api::{
    ApiCapabilities, ApiError, CeremonyEngineApi, CeremonyParticipant, CeremonySummary,
    DefinitionAnalysisView, DefinitionDefectView, InterventionResponseView, InterventionView,
    PublishedDefinitionView, RaiseInterventionRequest, RespondToInterventionRequest,
    StartCeremonyRequest, CONTRACT_VERSION,
};
use choreo_app::usecases::{
    RequestCeremonyInterventionInput, RespondToCeremonyInterventionInput, StartCeremonyInput,
};
use choreo_core::entities::CeremonyInstance;
use choreo_core::entities::CeremonyIntervention;
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    Attributes, AuditActorKind, CeremonyContext, CeremonyId, CeremonyInterventionContent,
    CeremonyInterventionId, CeremonyInterventionKind, CeremonyInterventionTarget, CeremonyName,
    CeremonyVersion, RoleId,
};
use time::OffsetDateTime;

use crate::{EmbeddedChoreographer, VERSION};

/// What this build can do, by name.
///
/// Listed here, next to the implementation, so that adding a method to the
/// trait without adding its name is a diff a reviewer sees in one place.
const CAPABILITIES: [&str; 7] = [
    "list_ceremonies",
    "get_ceremony",
    "start_ceremony",
    "raise_intervention",
    "respond_to_intervention",
    "analyze_definition",
    "publish_definition",
];

#[async_trait::async_trait]
impl CeremonyEngineApi for EmbeddedChoreographer {
    fn capabilities(&self) -> ApiCapabilities {
        ApiCapabilities::new(CONTRACT_VERSION, VERSION, CAPABILITIES)
    }

    async fn ceremonies(&self) -> Result<Vec<CeremonySummary>, ApiError> {
        let instances = self
            .instances()
            .await
            .map_err(|error| unavailable(&error))?;
        Ok(instances.iter().map(summarize).collect())
    }

    async fn ceremony(&self, ceremony_id: &str) -> Result<CeremonySummary, ApiError> {
        let id = CeremonyId::new(ceremony_id).map_err(|error| ApiError::Refused {
            reason: error.to_string(),
        })?;
        match self.instance(&id).await {
            Ok(instance) => Ok(summarize(&instance)),
            Err(DomainError::NotFound { .. }) => Err(ApiError::CeremonyNotFound {
                ceremony_id: ceremony_id.to_owned(),
            }),
            Err(error) => Err(unavailable(&error)),
        }
    }

    async fn raise_intervention(
        &self,
        request: RaiseInterventionRequest,
    ) -> Result<CeremonySummary, ApiError> {
        let input = raise_input(request).map_err(|error| ApiError::Refused {
            reason: error.to_string(),
        })?;
        match self.request_intervention(input).await {
            Ok(instance) => Ok(summarize(&instance)),
            Err(DomainError::NotFound { .. }) => Err(ApiError::CeremonyNotFound {
                ceremony_id: "the ceremony the intervention names".to_owned(),
            }),
            Err(error) => Err(ApiError::Refused {
                reason: error.to_string(),
            }),
        }
    }

    async fn respond_to_intervention(
        &self,
        request: RespondToInterventionRequest,
    ) -> Result<CeremonySummary, ApiError> {
        let input = respond_input(request).map_err(|error| ApiError::Refused {
            reason: error.to_string(),
        })?;
        match self.respond_to_intervention(input).await {
            Ok(instance) => Ok(summarize(&instance)),
            Err(DomainError::NotFound { .. }) => Err(ApiError::CeremonyNotFound {
                ceremony_id: "the ceremony the intervention names".to_owned(),
            }),
            Err(error) => Err(ApiError::Refused {
                reason: error.to_string(),
            }),
        }
    }

    async fn analyze_definition(
        &self,
        definition_yaml: &str,
    ) -> Result<DefinitionAnalysisView, ApiError> {
        let draft = CeremonyDefinitionYaml::parse_draft_str(definition_yaml).map_err(|error| {
            ApiError::Refused {
                reason: format!("that is not a definition at all: {error}"),
            }
        })?;
        let report = draft.analyze();
        Ok(DefinitionAnalysisView {
            publishable: report.is_valid(),
            defects: report
                .findings()
                .iter()
                .map(|finding| DefinitionDefectView {
                    severity: match finding.severity() {
                        choreo_core::value_objects::CeremonyValidationSeverity::Error => {
                            "error".to_owned()
                        }
                        choreo_core::value_objects::CeremonyValidationSeverity::Warning => {
                            "warning".to_owned()
                        }
                    },
                    locus: finding.locus().to_string(),
                    defect: finding.defect().to_string(),
                    blocking: finding.is_blocking(),
                })
                .collect(),
        })
    }

    async fn publish_definition(
        &self,
        definition_yaml: &str,
    ) -> Result<PublishedDefinitionView, ApiError> {
        let definition = CeremonyDefinitionYaml::parse_str(definition_yaml).map_err(|error| {
            ApiError::Refused {
                reason: format!("the definition does not construct: {error}"),
            }
        })?;
        match self.publish_definition(definition).await {
            Ok(choreo_core::entities::PublicationOutcome::Published(published)) => {
                Ok(published_view(&published, false))
            }
            Ok(choreo_core::entities::PublicationOutcome::AlreadyPublished(published)) => {
                Ok(published_view(&published, true))
            }
            Ok(choreo_core::entities::PublicationOutcome::VersionOccupied {
                published,
                offered,
            }) => Err(ApiError::Refused {
                reason: format!(
                    "that name and version already publish {}, not {}; a \
                     published version is immutable — publish a new version",
                    published.to_hex(),
                    offered.to_hex()
                ),
            }),
            Err(error) => Err(ApiError::Refused {
                reason: error.to_string(),
            }),
        }
    }

    async fn start_ceremony(
        &self,
        request: StartCeremonyRequest,
    ) -> Result<CeremonySummary, ApiError> {
        let ceremony_id = request.ceremony_id.clone();
        let input = start_input(request).map_err(|error| ApiError::Refused {
            reason: error.to_string(),
        })?;
        match self.start_published(input).await {
            Ok(instance) => Ok(summarize(&instance)),
            // Nothing published under that name and version. Publishing is the
            // remedy, not retrying.
            Err(DomainError::NotFound { .. }) => Err(ApiError::CeremonyNotFound {
                ceremony_id: format!("no published definition for `{ceremony_id}`"),
            }),
            // Everything else the domain says here is about the request — a
            // taken identity, a defective field. Refused, so nobody retries an
            // answer that will not change.
            Err(error) => Err(ApiError::Refused {
                reason: error.to_string(),
            }),
        }
    }
}

/// Parse the plain request into the domain's terms — the one place a consumer's
/// strings meet the engine's validation.
fn start_input(request: StartCeremonyRequest) -> Result<StartCeremonyInput, DomainError> {
    let actor_kind = parse_actor_kind(&request.actor_kind)?;
    Ok(StartCeremonyInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyName::new(request.definition_name)?,
        CeremonyVersion::new(request.definition_version)?,
        CeremonyContext::new(Attributes::new(BTreeMap::from_iter(request.context))?),
        request.actor_id,
        actor_kind,
    ))
}

fn summarize(instance: &CeremonyInstance) -> CeremonySummary {
    CeremonySummary {
        ceremony_id: instance.id().as_str().to_owned(),
        definition_name: instance.definition_name().as_str().to_owned(),
        definition_version: instance.definition_version().as_str().to_owned(),
        definition_digest: instance
            .bound_definition()
            .map(choreo_core::value_objects::CeremonyDefinitionDigest::to_hex),
        current_state: instance.current_state().as_str().to_owned(),
        interventions: instance
            .interventions()
            .iter()
            .map(intervention_view)
            .collect(),
        participants: instance
            .participant_bindings()
            .values()
            .map(|binding| CeremonyParticipant {
                role_id: binding.role_id().as_str().to_owned(),
                specialty: binding.specialty().as_str().to_owned(),
                bound_at_millis: millis(binding.bound_at()),
            })
            .collect(),
        context: instance.context().attributes().as_map().clone(),
        created_at_millis: millis(instance.created_at()),
        updated_at_millis: millis(instance.updated_at()),
        completed_at_millis: instance.completed_at().map(millis),
    }
}

fn published_view(
    published: &choreo_core::entities::PublishedCeremonyDefinition,
    already_published: bool,
) -> PublishedDefinitionView {
    PublishedDefinitionView {
        name: published.name().as_str().to_owned(),
        version: published.version().as_str().to_owned(),
        digest: published.digest().to_hex(),
        already_published,
    }
}

fn parse_actor_kind(raw: &str) -> Result<AuditActorKind, DomainError> {
    match raw {
        "human" => Ok(AuditActorKind::Human),
        "agent" => Ok(AuditActorKind::Agent),
        "service" => Ok(AuditActorKind::Service),
        "engine" => Ok(AuditActorKind::Engine),
        _ => Err(DomainError::InvalidCharacters {
            field: "actor_kind",
        }),
    }
}

fn parse_intervention_kind(raw: &str) -> Result<CeremonyInterventionKind, DomainError> {
    match raw {
        "opinion" => Ok(CeremonyInterventionKind::Opinion),
        "investigation" => Ok(CeremonyInterventionKind::Investigation),
        "action" => Ok(CeremonyInterventionKind::Action),
        _ => Err(DomainError::InvalidCharacters {
            field: "intervention_kind",
        }),
    }
}

fn parse_target(role_ids: Vec<String>) -> Result<CeremonyInterventionTarget, DomainError> {
    if role_ids.is_empty() {
        return Ok(CeremonyInterventionTarget::Table);
    }
    let roles = role_ids
        .into_iter()
        .map(RoleId::new)
        .collect::<Result<Vec<_>, _>>()?;
    CeremonyInterventionTarget::roles(roles)
}

fn raise_input(
    request: RaiseInterventionRequest,
) -> Result<RequestCeremonyInterventionInput, DomainError> {
    Ok(RequestCeremonyInterventionInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyInterventionId::new(request.intervention_id)?,
        RoleId::new(request.role_id)?,
        parse_actor_kind(&request.role_kind)?,
        parse_intervention_kind(&request.kind)?,
        parse_target(request.target_role_ids)?,
        CeremonyInterventionContent::new(request.request, Attributes::empty())?,
    ))
}

fn respond_input(
    request: RespondToInterventionRequest,
) -> Result<RespondToCeremonyInterventionInput, DomainError> {
    Ok(RespondToCeremonyInterventionInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyInterventionId::new(request.intervention_id)?,
        RoleId::new(request.role_id)?,
        parse_actor_kind(&request.role_kind)?,
        CeremonyInterventionContent::new(request.content, Attributes::empty())?,
    ))
}

fn intervention_view(intervention: &CeremonyIntervention) -> InterventionView {
    InterventionView {
        intervention_id: intervention.id().as_str().to_owned(),
        kind: intervention.kind().as_label().to_owned(),
        requested_by: intervention.requested_by().as_str().to_owned(),
        target_role_ids: match intervention.target() {
            CeremonyInterventionTarget::Table => Vec::new(),
            CeremonyInterventionTarget::Roles(roles) => {
                roles.iter().map(|role| role.as_str().to_owned()).collect()
            }
        },
        request: intervention.request().message().to_owned(),
        open: intervention.status().is_open(),
        responses: intervention
            .responses()
            .iter()
            .map(|response| InterventionResponseView {
                role_id: response.role_id().as_str().to_owned(),
                content: response.content().message().to_owned(),
                responded_at_millis: millis(response.responded_at()),
            })
            .collect(),
        created_at_millis: millis(intervention.created_at()),
        closed_at_millis: intervention.closed_at().map(millis),
    }
}

fn unavailable(error: &DomainError) -> ApiError {
    ApiError::Unavailable {
        reason: error.to_string(),
    }
}

fn millis(at: OffsetDateTime) -> i64 {
    (at.unix_timestamp_nanos() / 1_000_000) as i64
}
