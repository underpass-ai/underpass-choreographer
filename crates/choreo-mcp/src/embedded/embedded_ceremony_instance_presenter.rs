use choreo_app::usecases::CeremonyInstanceView;
use choreo_core::entities::CeremonyInstance;
use choreo_core::value_objects::{
    CeremonyDefinitionDigest, CeremonyId, CeremonyRecordRef, RoleId, StepId,
};
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
        // Derived once in the application layer and rendered here. The
        // gRPC adapter renders the same view, which is what keeps one
        // working session from looking like two depending on how a
        // client reached it.
        let view = CeremonyInstanceView::project(&instance, &definition)
            .map_err(|error| format!("ceremony instance could not be projected: {error}"))?;

        let steps = view
            .steps()
            .iter()
            .map(|step| {
                json!({
                    "step_id": step.step().id().as_str(),
                    "state_id": step.step().state_id().as_str(),
                    "status": step.record().status().as_label(),
                    "attempt": step.record().attempt().get(),
                    "output": step.record().output().attributes().as_map(),
                    "error": step.record().error_message().map(ToString::to_string),
                })
            })
            .collect::<Vec<_>>();
        let transitions = view
            .transitions()
            .iter()
            .map(|transition| {
                json!({
                    "trigger": transition.transition().trigger().as_str(),
                    "to_state": transition.transition().to().as_str(),
                    "enabled": transition.is_enabled(),
                    "guards": transition
                        .guards()
                        .iter()
                        .map(|guard| json!({
                            "name": guard.name().as_str(),
                            "kind": if guard.is_human() { "human" } else { "automated" },
                            "satisfied": guard.is_satisfied(),
                        }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();
        let waiting_for_human = view
            .waiting_for_human()
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>();
        let next_step_id = view.next_step_id().map(StepId::as_str);
        let interventions = intervention_values(&instance);
        let open_intervention_ids = open_intervention_ids(&instance);
        let guard_deferrals = guard_deferral_values(&instance);

        Ok(json!({
            "ceremony_id": instance.id().as_str(),
            "definition_name": definition.name().as_str(),
            "definition_version": definition.version().as_str(),
            // Present whether this instance runs a definition that can
            // be looked up and checked, or one supplied for the run.
            // Binding it and not saying so would leave the caller
            // unable to tell the two apart.
            "bound_definition_digest": instance
                .bound_definition()
                .map(CeremonyDefinitionDigest::to_hex),
            "current_state": instance.current_state().as_str(),
            "completed": view.is_completed(),
            "next_step_id": next_step_id,
            "waiting_for_human": waiting_for_human,
            "guard_deferrals": guard_deferrals,
            "transitions": transitions,
            "steps": steps,
            "interventions": interventions,
            "open_intervention_ids": open_intervention_ids,
            "context": instance.context(),
            "participant_bindings": view
                .participant_bindings()
                .values()
                .map(|binding| json!({
                    "role_id": binding.role_id().as_str(),
                    "specialty": binding.specialty().as_str(),
                    "bound_at": binding.bound_at(),
                }))
                .collect::<Vec<_>>(),
            // Read back, not only written: a seat that explained this
            // session can find its own explanation again. Until now the
            // edges crossed the wire in one direction only.
            "reasons": view
                .reasons()
                .iter()
                .map(|reason| json!({
                    "from": record_ref_value(reason.from()),
                    "to": record_ref_value(reason.to()),
                    "kind": reason.kind().as_label(),
                    "why": reason.why(),
                    "confidence": reason.confidence().as_label(),
                    "asserted_by_role_id": reason.asserted_by().map(RoleId::as_str),
                    "asserted_at": reason.asserted_at(),
                }))
                .collect::<Vec<_>>(),
        }))
    }
}

/// What a reason points at, flat with a discriminator.
fn record_ref_value(record: &CeremonyRecordRef) -> Value {
    match record {
        CeremonyRecordRef::Step { step_id } => json!({
            "kind": "step", "step_id": step_id.as_str()
        }),
        CeremonyRecordRef::AgendaItem { agenda_item } => json!({
            "kind": "agenda_item", "agenda_item": agenda_item.as_str()
        }),
        CeremonyRecordRef::Contribution {
            agenda_item,
            ordinal,
        } => json!({
            "kind": "contribution", "agenda_item": agenda_item.as_str(), "ordinal": ordinal
        }),
        CeremonyRecordRef::GuardDecision { guard_name } => json!({
            "kind": "guard_decision", "guard_name": guard_name.as_str()
        }),
        CeremonyRecordRef::Transition { ordinal } => json!({
            "kind": "transition", "ordinal": ordinal
        }),
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
                        "evidence_pack": response.evidence_pack(),
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
