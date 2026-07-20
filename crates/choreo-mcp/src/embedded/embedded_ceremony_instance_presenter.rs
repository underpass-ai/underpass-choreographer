use choreo_core::value_objects::{CeremonyId, GuardCondition};
use choreo_embedded::EmbeddedChoreographer;
use serde_json::{json, Value};

use super::embedded_request_fields::load_instance_definition;

/// Projects the current persistent ceremony state onto the MCP wire contract.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct EmbeddedCeremonyInstancePresenter;

impl EmbeddedCeremonyInstancePresenter {
    pub(super) async fn present(
        choreographer: &EmbeddedChoreographer,
        ceremony_id: &CeremonyId,
    ) -> Result<Value, String> {
        let (definition, instance) = load_instance_definition(choreographer, ceremony_id).await?;
        let steps = definition
            .steps_in_declaration_order()
            .map(|step| {
                let record = instance
                    .step_record(step.id())
                    .expect("started ceremony must have one record per declared step");
                json!({
                    "step_id": step.id().as_str(),
                    "state_id": step.state_id().as_str(),
                    "status": record.status().as_label(),
                    "attempt": record.attempt().get(),
                    "output": record.output().attributes().as_map(),
                    "error": record.error_message().map(ToString::to_string),
                })
            })
            .collect::<Vec<_>>();
        let transitions = definition
            .available_transitions(instance.current_state())
            .map(|transition| {
                let guards = transition
                    .required_guards()
                    .iter()
                    .map(|guard_name| {
                        let guard = definition
                            .guards()
                            .get(guard_name)
                            .expect("validated transition must reference a declared guard");
                        json!({
                            "name": guard_name.as_str(),
                            "kind": if matches!(guard.condition(), GuardCondition::HumanApproval) {
                                "human"
                            } else {
                                "automated"
                            },
                            "satisfied": guard.is_satisfied(
                                instance.step_records(),
                                instance.context(),
                            ),
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "trigger": transition.trigger().as_str(),
                    "to_state": transition.to().as_str(),
                    "enabled": definition.guards_are_satisfied(
                        transition,
                        instance.step_records(),
                        instance.context(),
                    ),
                    "guards": guards,
                })
            })
            .collect::<Vec<_>>();
        let waiting_for_human = transitions
            .iter()
            .flat_map(|transition| transition["guards"].as_array().into_iter().flatten())
            .filter(|guard| guard["kind"] == "human" && guard["satisfied"] == false)
            .filter_map(|guard| guard["name"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let next_step_id = definition
            .steps_for_state(instance.current_state())
            .find(|step| {
                instance
                    .step_record(step.id())
                    .is_some_and(|record| !record.status().is_success())
            })
            .map(|step| step.id().as_str());

        Ok(json!({
            "ceremony_id": instance.id().as_str(),
            "definition_name": definition.name().as_str(),
            "definition_version": definition.version().as_str(),
            "current_state": instance.current_state().as_str(),
            "completed": instance.is_completed(&definition),
            "next_step_id": next_step_id,
            "waiting_for_human": waiting_for_human,
            "transitions": transitions,
            "steps": steps,
            "context": instance.context(),
        }))
    }
}
