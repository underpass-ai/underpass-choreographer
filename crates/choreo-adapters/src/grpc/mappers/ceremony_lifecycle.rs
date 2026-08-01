//! Advancing a ceremony one move at a time: proto → application.
//!
//! These conversions are deliberately thin. Everything they need to
//! decide — which role may run a step, which role may fire a trigger —
//! is asked of the definition, which is where those rules already
//! live. An adapter that answered them itself would be a second copy
//! of the engine's authorization rules, drifting quietly.

use choreo_app::usecases::{
    ApplyCeremonyTransitionInput, RunCeremonyStepInput, StartCeremonyInput,
};
use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    CeremonyContext, CeremonyId, CeremonyName, CeremonyVersion, DurationMs, IdempotencyKey,
    LeaseOwnerId, StepId, TransitionTrigger,
};
use choreo_proto::v1 as pb;
use uuid::Uuid;

use crate::yaml::CeremonyDefinitionYaml;

use super::actor_kind::actor_kind_from_proto;
use super::attributes::attributes_from_struct;

const DEFAULT_LEASE_OWNER_ID: &str = "grpc-run-ceremony-step";
const DEFAULT_LEASE_TTL_MS: u64 = 30_000;

/// A ceremony started from YAML supplied for the run. The definition
/// travels with the input because the caller must persist it before
/// the session can be advanced: a step asked for tomorrow has to find
/// the definition it belongs to.
pub struct StartCeremonyFromYaml {
    pub definition: CeremonyDefinition,
    pub input: StartCeremonyInput,
}

pub fn start_ceremony_from_proto(
    request: pb::StartCeremonyRequest,
) -> Result<StartCeremonyFromYaml, DomainError> {
    let id = CeremonyId::new(request.ceremony_id)?;
    let definition = CeremonyDefinitionYaml::parse_str(&request.definition_yaml)?;
    let context = CeremonyContext::new(attributes_from_struct(request.context)?);
    let input = StartCeremonyInput::new(
        id,
        definition.name().clone(),
        definition.version().clone(),
        context,
        request.actor_id,
        actor_kind_from_proto(&request.actor_kind, "actor_kind")?,
    );
    Ok(StartCeremonyFromYaml { definition, input })
}

pub fn start_published_ceremony_input_from_proto(
    request: pb::StartPublishedCeremonyRequest,
) -> Result<StartCeremonyInput, DomainError> {
    Ok(StartCeremonyInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyName::new(request.ceremony)?,
        CeremonyVersion::new(request.version)?,
        CeremonyContext::new(attributes_from_struct(request.context)?),
        request.actor_id,
        actor_kind_from_proto(&request.actor_kind, "actor_kind")?,
    ))
}

/// The session is named, never the definition. What an instance runs
/// is settled when it starts — and resolved from the instance by the
/// use case — so there is nothing here a caller could use to point a
/// running session at a definition of their choosing.
pub fn run_ceremony_step_input_from_proto(
    request: pb::RunCeremonyStepRequest,
    definition: &CeremonyDefinition,
    instance: &CeremonyInstance,
) -> Result<RunCeremonyStepInput, DomainError> {
    let step_id = StepId::new(request.step_id)?;
    // The seat is the definition's to say; what filled it is the
    // caller's. Taking both from the definition would record a kind
    // nobody declared.
    let role_id = definition.role_id_for_step(&step_id)?;
    let role_kind = actor_kind_from_proto(&request.actor_kind, "actor_kind")?;
    let lease_owner_id = if request.lease_owner_id.trim().is_empty() {
        LeaseOwnerId::new(DEFAULT_LEASE_OWNER_ID)?
    } else {
        LeaseOwnerId::new(request.lease_owner_id)?
    };
    let idempotency_key = if request.idempotency_key.trim().is_empty() {
        IdempotencyKey::new(format!("grpc-{}", Uuid::new_v4()))?
    } else {
        IdempotencyKey::new(request.idempotency_key)?
    };
    let lease_ttl = if request.lease_ttl_ms == 0 {
        DurationMs::from_millis(DEFAULT_LEASE_TTL_MS)
    } else {
        DurationMs::from_millis(request.lease_ttl_ms)
    };

    Ok(RunCeremonyStepInput::new(
        instance.id().clone(),
        role_id,
        role_kind,
        step_id,
        lease_owner_id,
        idempotency_key,
        lease_ttl,
    ))
}

pub fn apply_ceremony_transition_input_from_proto(
    request: pb::ApplyCeremonyTransitionRequest,
    definition: &CeremonyDefinition,
    instance: &CeremonyInstance,
) -> Result<ApplyCeremonyTransitionInput, DomainError> {
    let trigger = TransitionTrigger::new(request.trigger)?;
    // The seat is the definition's to say; what filled it is the
    // caller's. Taking both from the definition would record a kind
    // nobody declared.
    let role_id = definition.role_id_for_transition(&trigger)?;
    let role_kind = actor_kind_from_proto(&request.actor_kind, "actor_kind")?;
    Ok(ApplyCeremonyTransitionInput::new(
        instance.id().clone(),
        role_id,
        role_kind,
        trigger,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    const EDITORIAL_MEETING_CEREMONY: &str =
        include_str!("../../../../../tests/e2e/ceremonies/editorial-planning-meeting.yaml");

    fn definition() -> CeremonyDefinition {
        CeremonyDefinitionYaml::parse_str(EDITORIAL_MEETING_CEREMONY).unwrap()
    }

    fn started_instance(definition: &CeremonyDefinition) -> CeremonyInstance {
        CeremonyInstance::start(
            CeremonyId::new("ceremony-lifecycle-1").unwrap(),
            definition,
            CeremonyContext::empty(),
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    fn step_request(step_id: &str) -> pb::RunCeremonyStepRequest {
        pb::RunCeremonyStepRequest {
            ceremony_id: "ceremony-lifecycle-1".to_owned(),
            actor_kind: "agent".to_owned(),
            step_id: step_id.to_owned(),
            lease_owner_id: String::new(),
            idempotency_key: String::new(),
            lease_ttl_ms: 0,
        }
    }

    #[test]
    fn a_step_is_run_as_the_role_the_definition_authorizes() {
        let definition = definition();
        let instance = started_instance(&definition);

        let input =
            run_ceremony_step_input_from_proto(step_request("open_room"), &definition, &instance)
                .unwrap();

        assert_eq!(input.role_id().as_str(), "FACILITATOR");
        assert_eq!(input.lease_owner_id().as_str(), DEFAULT_LEASE_OWNER_ID);
        assert_eq!(input.lease_ttl().get(), DEFAULT_LEASE_TTL_MS);
        // An empty key still produces one, so nothing downstream has
        // to cope with its absence.
        assert!(input.idempotency_key().as_str().starts_with("grpc-"));
    }

    #[test]
    fn a_step_no_role_is_authorized_for_is_refused_before_it_runs() {
        let definition = definition();
        let instance = started_instance(&definition);

        let error =
            run_ceremony_step_input_from_proto(step_request("not_a_step"), &definition, &instance)
                .unwrap_err();

        assert!(matches!(error, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn an_explicit_lease_and_key_are_carried_through_untouched() {
        let definition = definition();
        let instance = started_instance(&definition);

        let input = run_ceremony_step_input_from_proto(
            pb::RunCeremonyStepRequest {
                ceremony_id: "ceremony-lifecycle-1".to_owned(),
                actor_kind: "agent".to_owned(),
                step_id: "open_room".to_owned(),
                lease_owner_id: "operator-7".to_owned(),
                idempotency_key: "retry-42".to_owned(),
                lease_ttl_ms: 5_000,
            },
            &definition,
            &instance,
        )
        .unwrap();

        assert_eq!(input.lease_owner_id().as_str(), "operator-7");
        assert_eq!(input.idempotency_key().as_str(), "retry-42");
        assert_eq!(input.lease_ttl().get(), 5_000);
    }

    #[test]
    fn the_session_acted_on_is_the_one_loaded_not_the_one_named() {
        let definition = definition();
        let instance = started_instance(&definition);

        let input = apply_ceremony_transition_input_from_proto(
            pb::ApplyCeremonyTransitionRequest {
                actor_kind: "agent".to_owned(),
                // Naming another ceremony here must not move this
                // session onto it.
                ceremony_id: "some-other-ceremony".to_owned(),
                trigger: "context_shared".to_owned(),
            },
            &definition,
            &instance,
        )
        .unwrap();

        assert_eq!(input.instance_id(), instance.id());
    }
}
