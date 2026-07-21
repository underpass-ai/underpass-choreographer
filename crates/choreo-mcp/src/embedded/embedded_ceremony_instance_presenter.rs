use choreo_core::entities::CeremonyInstance;
use choreo_core::value_objects::{CeremonyId, GuardCondition, RoleId};
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
            .filter(|transition| {
                transition["guards"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|guard| guard["kind"] == "automated")
                    .all(|guard| guard["satisfied"] == true)
            })
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
        let interventions = intervention_values(&instance);
        let open_intervention_ids = open_intervention_ids(&instance);
        let guard_deferrals = guard_deferral_values(&instance);

        Ok(json!({
            "ceremony_id": instance.id().as_str(),
            "definition_name": definition.name().as_str(),
            "definition_version": definition.version().as_str(),
            "current_state": instance.current_state().as_str(),
            "completed": instance.is_completed(&definition),
            "next_step_id": next_step_id,
            "waiting_for_human": waiting_for_human,
            "guard_deferrals": guard_deferrals,
            "transitions": transitions,
            "steps": steps,
            "interventions": interventions,
            "open_intervention_ids": open_intervention_ids,
            "context": instance.context(),
        }))
    }
}

fn guard_deferral_values(instance: &CeremonyInstance) -> Vec<Value> {
    instance
        .guard_deferrals()
        .iter()
        .map(|deferral| {
            json!({
                "guard_name": deferral.guard_name().as_str(),
                "statement": deferral.content().statement(),
                "reason": deferral.content().reason(),
                "reconsider_when": deferral.content().reconsider_when(),
                "deferred_at": deferral.deferred_at(),
            })
        })
        .collect()
}

fn intervention_values(instance: &CeremonyInstance) -> Vec<Value> {
    instance
        .interventions()
        .iter()
        .map(|intervention| {
            let target = intervention.target().role_ids().map_or_else(
                || json!({ "kind": "table" }),
                |role_ids| {
                    json!({
                        "kind": "roles",
                        "role_ids": role_ids.iter().map(RoleId::as_str).collect::<Vec<_>>(),
                    })
                },
            );
            let responses = intervention
                .responses()
                .iter()
                .map(|response| {
                    json!({
                        "role_id": response.role_id().as_str(),
                        "message": response.content().message(),
                        "details": response.content().details().as_map(),
                        "responded_at": response.responded_at(),
                    })
                })
                .collect::<Vec<_>>();
            let provenance = intervention.provenance().map(|provenance| {
                json!({
                    "source_intervention_id": provenance.source_intervention_id().as_str(),
                    "source_response_role_id": provenance.source_response_role_id().as_str(),
                    "selected_role_id": provenance.selected_role_id().as_str(),
                })
            });
            json!({
                "intervention_id": intervention.id().as_str(),
                "kind": intervention.kind().as_label(),
                "status": intervention.status().as_label(),
                "requested_by": intervention.requested_by().as_str(),
                "target": target,
                "request": {
                    "message": intervention.request().message(),
                    "details": intervention.request().details().as_map(),
                },
                "provenance": provenance,
                "responses": responses,
                "created_at": intervention.created_at(),
                "updated_at": intervention.updated_at(),
                "closed_at": intervention.closed_at(),
            })
        })
        .collect()
}

fn open_intervention_ids(instance: &CeremonyInstance) -> Vec<&str> {
    instance
        .interventions()
        .iter()
        .filter(|intervention| intervention.status().is_open())
        .map(|intervention| intervention.id().as_str())
        .collect()
}
