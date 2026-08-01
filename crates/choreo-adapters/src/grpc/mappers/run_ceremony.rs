//! Ceremony execution: proto ↔ application.

use choreo_app::usecases::{RunCeremonyInput, RunCeremonyOutput};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    CeremonyContext, CeremonyId, DurationMs, LeaseOwnerId, StepStatus,
};
use choreo_proto::v1 as pb;
use uuid::Uuid;

use crate::mermaid::CeremonyConversationDiagram;
use crate::yaml::CeremonyDefinitionYaml;

use super::actor_kind::actor_kind_from_proto;
use super::attributes::attributes_from_struct;

const DEFAULT_LEASE_OWNER_ID: &str = "grpc-run-ceremony";
const DEFAULT_LEASE_TTL_MS: u64 = 60_000;

pub fn run_ceremony_input_from_proto(
    request: pb::RunCeremonyRequest,
) -> Result<RunCeremonyInput, DomainError> {
    let id = if request.ceremony_id.trim().is_empty() {
        CeremonyId::new(Uuid::new_v4().to_string())?
    } else {
        CeremonyId::new(request.ceremony_id)?
    };
    let definition = CeremonyDefinitionYaml::parse_str(&request.definition_yaml)?;
    let context = CeremonyContext::new(attributes_from_struct(request.context)?);
    let lease_owner_id = if request.lease_owner_id.trim().is_empty() {
        LeaseOwnerId::new(DEFAULT_LEASE_OWNER_ID)?
    } else {
        LeaseOwnerId::new(request.lease_owner_id)?
    };
    let lease_ttl = if request.lease_ttl_ms == 0 {
        DurationMs::from_millis(DEFAULT_LEASE_TTL_MS)
    } else {
        DurationMs::from_millis(request.lease_ttl_ms)
    };

    Ok(RunCeremonyInput::new(
        id,
        definition,
        context,
        lease_owner_id,
        lease_ttl,
        request.actor_id,
        actor_kind_from_proto(&request.actor_kind, "actor_kind")?,
    ))
}

pub fn run_ceremony_response_from(output: &RunCeremonyOutput) -> pb::RunCeremonyResponse {
    let instance = output.instance();
    let definition = output.definition();
    pb::RunCeremonyResponse {
        ceremony_id: instance.id().as_str().to_owned(),
        definition_name: definition.name().as_str().to_owned(),
        definition_version: definition.version().as_str().to_owned(),
        final_state: instance.current_state().as_str().to_owned(),
        completed: instance.is_completed(definition),
        steps: output
            .step_traces()
            .iter()
            .map(|trace| pb::CeremonyStepExecution {
                state_id: trace.state_id().as_str().to_owned(),
                step_id: trace.step_id().as_str().to_owned(),
                role_id: trace.role_id().as_str().to_owned(),
                status: step_status_to_proto(trace.status()).to_owned(),
                attempt: trace.attempt().get(),
                // Surface the deliberation winner's content so the response
                // carries the meeting's record, not just step statuses.
                output: trace
                    .output()
                    .attributes()
                    .get(crate::ceremony::WINNER_CONTENT_KEY)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect(),
        mermaid_sequence: CeremonyConversationDiagram::render(definition),
    }
}

fn step_status_to_proto(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Pending => "PENDING",
        StepStatus::InProgress => "IN_PROGRESS",
        StepStatus::Completed => "COMPLETED",
        StepStatus::Failed => "FAILED",
        StepStatus::WaitingForHuman => "WAITING_FOR_HUMAN",
        StepStatus::Cancelled => "CANCELLED",
    }
}

#[cfg(test)]
mod tests {
    use choreo_core::value_objects::{CeremonyName, StepOutput, StepResult};

    use super::*;
    use choreo_app::usecases::CeremonyStepTrace;
    use choreo_core::entities::CeremonyInstance;
    use choreo_core::value_objects::{RoleId, StateId, StepAttempt, StepId};
    use time::macros::datetime;

    const CEREMONY: &str = r#"
version: "1.0"
name: "rpc_ceremony"
states:
  - id: STARTED
    initial: true
  - id: DONE
    terminal: true
transitions:
  - from: STARTED
    to: DONE
    trigger: finish
    guards:
      - work_completed
steps:
  - id: work
    state: STARTED
    handler: noop_step
guards:
  work_completed:
    type: automated
    check: "step_status:work:COMPLETED"
roles:
  - id: RUNNER
    allowed_actions:
      - work
      - finish
"#;

    #[test]
    fn request_maps_yaml_to_run_ceremony_input() {
        let input = run_ceremony_input_from_proto(pb::RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "ceremony-rpc-1".to_owned(),
            definition_yaml: CEREMONY.to_owned(),
            context: None,
            lease_owner_id: String::new(),
            lease_ttl_ms: 0,
        })
        .unwrap();

        assert_eq!(input.id().as_str(), "ceremony-rpc-1");
        assert_eq!(
            input.definition().name(),
            &CeremonyName::new("rpc_ceremony").unwrap()
        );
        assert_eq!(input.lease_owner_id().as_str(), DEFAULT_LEASE_OWNER_ID);
        assert_eq!(input.lease_ttl().get(), DEFAULT_LEASE_TTL_MS);
    }

    #[test]
    fn response_projects_step_trace_and_diagram() {
        let definition = CeremonyDefinitionYaml::parse_str(CEREMONY).unwrap();
        let mut instance = CeremonyInstance::start(
            CeremonyId::new("ceremony-rpc-1").unwrap(),
            &definition,
            CeremonyContext::empty(),
            datetime!(2026-06-06 12:00:00 UTC),
        );
        let step_id = StepId::new("work").unwrap();
        instance
            .start_step(
                &definition,
                &step_id,
                choreo_core::value_objects::StepLease::new(
                    LeaseOwnerId::new("runner").unwrap(),
                    choreo_core::value_objects::IdempotencyKey::new("key-1").unwrap(),
                    datetime!(2026-06-06 12:00:00 UTC),
                    datetime!(2026-06-06 12:01:00 UTC),
                )
                .unwrap(),
                datetime!(2026-06-06 12:00:00 UTC),
            )
            .unwrap();
        instance
            .apply_step_result(
                &definition,
                &step_id,
                StepResult::completed(StepOutput::empty()).unwrap(),
                datetime!(2026-06-06 12:00:00 UTC),
            )
            .unwrap();
        instance
            .apply_transition(
                &definition,
                &choreo_core::value_objects::TransitionTrigger::new("finish").unwrap(),
                datetime!(2026-06-06 12:00:00 UTC),
            )
            .unwrap();
        let output = RunCeremonyOutput::new(
            definition,
            instance,
            vec![CeremonyStepTrace::new(
                StateId::new("STARTED").unwrap(),
                StepId::new("work").unwrap(),
                RoleId::new("RUNNER").unwrap(),
                StepAttempt::FIRST,
                StepStatus::Completed,
                StepOutput::new(
                    choreo_core::value_objects::Attributes::new(std::collections::BTreeMap::from(
                        [(
                            crate::ceremony::WINNER_CONTENT_KEY.to_owned(),
                            serde_json::json!("the agreed build plan"),
                        )],
                    ))
                    .unwrap(),
                ),
            )],
        );

        let response = run_ceremony_response_from(&output);

        assert!(response.completed);
        assert_eq!(response.final_state, "DONE");
        assert_eq!(response.steps.len(), 1);
        assert_eq!(response.steps[0].status, "COMPLETED");
        assert_eq!(response.steps[0].output, "the agreed build plan");
        assert!(response.mermaid_sequence.contains("sequenceDiagram"));
    }
}
