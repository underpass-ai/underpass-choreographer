//! [`EmbeddedChoreographer`] as an implementation of the published contract.
//!
//! The conversion in this module is the whole of the coupling a consumer is
//! allowed: domain aggregate in, plain view out. Nothing of `choreo-core`
//! crosses the trait.

use choreo_api::{
    ApiCapabilities, ApiError, CeremonyEngineApi, CeremonyParticipant, CeremonySummary,
    CONTRACT_VERSION,
};
use choreo_core::entities::CeremonyInstance;
use choreo_core::error::DomainError;
use choreo_core::value_objects::CeremonyId;
use time::OffsetDateTime;

use crate::{EmbeddedChoreographer, VERSION};

/// What this build can do, by name.
///
/// Listed here, next to the implementation, so that adding a method to the
/// trait without adding its name is a diff a reviewer sees in one place.
const CAPABILITIES: [&str; 2] = ["list_ceremonies", "get_ceremony"];

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

fn unavailable(error: &DomainError) -> ApiError {
    ApiError::Unavailable {
        reason: error.to_string(),
    }
}

fn millis(at: OffsetDateTime) -> i64 {
    (at.unix_timestamp_nanos() / 1_000_000) as i64
}
