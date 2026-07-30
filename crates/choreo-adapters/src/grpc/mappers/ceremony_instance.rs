//! Live ceremony state: application view → proto.
//!
//! Renders the same [`CeremonyInstanceView`] the embedded adapter
//! renders to JSON. Neither side derives anything of its own, which is
//! what makes "the same working session over either transport" a
//! property of the code rather than a promise in a document.

use choreo_app::usecases::{CeremonyInstanceView, CeremonyStepView, CeremonyTransitionView};
use choreo_core::entities::{CeremonyInstance, CeremonyIntervention};
use choreo_core::value_objects::{
    CeremonyDefinitionDigest, CeremonyGuardDeferral, CeremonyInterventionResponse, RoleId, StepId,
};
use choreo_proto::v1 as pb;

use super::attributes::attributes_to_struct;

pub fn ceremony_instance_state_from(view: &CeremonyInstanceView<'_>) -> pb::CeremonyInstanceState {
    let instance = view.instance();

    pb::CeremonyInstanceState {
        ceremony_id: instance.id().as_str().to_owned(),
        definition_name: instance.definition_name().as_str().to_owned(),
        definition_version: instance.definition_version().as_str().to_owned(),
        // Empty rather than absent: proto3 has no null, and an instance
        // started from a definition supplied for the run must not look
        // like one bound to a published version.
        bound_definition_digest: instance
            .bound_definition()
            .map(CeremonyDefinitionDigest::to_hex)
            .unwrap_or_default(),
        current_state: instance.current_state().as_str().to_owned(),
        completed: view.is_completed(),
        next_step_id: view
            .next_step_id()
            .map(StepId::as_str)
            .unwrap_or_default()
            .to_owned(),
        waiting_for_human: view
            .waiting_for_human()
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect(),
        guard_deferrals: instance
            .guard_deferrals()
            .iter()
            .map(guard_deferral_state_from)
            .collect(),
        transitions: view
            .transitions()
            .iter()
            .map(transition_state_from)
            .collect(),
        steps: view.steps().iter().map(step_state_from).collect(),
        interventions: instance
            .interventions()
            .iter()
            .map(intervention_state_from)
            .collect(),
        open_intervention_ids: open_intervention_ids(instance),
        context: Some(attributes_to_struct(instance.context().attributes())),
    }
}

fn step_state_from(step: &CeremonyStepView<'_>) -> pb::CeremonyStepState {
    pb::CeremonyStepState {
        step_id: step.step().id().as_str().to_owned(),
        state_id: step.step().state_id().as_str().to_owned(),
        status: step.record().status().as_label().to_owned(),
        attempt: step.record().attempt().get(),
        output: Some(attributes_to_struct(step.record().output().attributes())),
        error: step
            .record()
            .error_message()
            .map(ToString::to_string)
            .unwrap_or_default(),
    }
}

fn transition_state_from(
    transition: &CeremonyTransitionView<'_>,
) -> pb::CeremonyAvailableTransition {
    pb::CeremonyAvailableTransition {
        trigger: transition.transition().trigger().as_str().to_owned(),
        to: transition.transition().to().as_str().to_owned(),
        enabled: transition.is_enabled(),
        guards: transition
            .guards()
            .iter()
            .map(|guard| pb::CeremonyTransitionGuard {
                name: guard.name().as_str().to_owned(),
                kind: if guard.is_human() {
                    "human"
                } else {
                    "automated"
                }
                .to_owned(),
                satisfied: guard.is_satisfied(),
            })
            .collect(),
    }
}

fn guard_deferral_state_from(deferral: &CeremonyGuardDeferral) -> pb::CeremonyGuardDeferralState {
    pb::CeremonyGuardDeferralState {
        guard_name: deferral.guard_name().as_str().to_owned(),
        statement: deferral.content().statement().to_owned(),
        reason: deferral.content().reason().to_owned(),
        reconsider_when: deferral.content().reconsider_when().to_vec(),
        deferred_at: deferral.deferred_at().to_string(),
    }
}

fn intervention_state_from(intervention: &CeremonyIntervention) -> pb::CeremonyInterventionState {
    pb::CeremonyInterventionState {
        intervention_id: intervention.id().as_str().to_owned(),
        kind: intervention.kind().as_label().to_owned(),
        status: intervention.status().as_label().to_owned(),
        requested_by: intervention.requested_by().as_str().to_owned(),
        target: Some(match intervention.target().role_ids() {
            Some(role_ids) => pb::CeremonyInterventionTargetState {
                kind: "roles".to_owned(),
                role_ids: role_ids
                    .iter()
                    .map(RoleId::as_str)
                    .map(str::to_owned)
                    .collect(),
            },
            None => pb::CeremonyInterventionTargetState {
                kind: "table".to_owned(),
                role_ids: Vec::new(),
            },
        }),
        request: Some(pb::CeremonyInterventionMessage {
            message: intervention.request().message().to_owned(),
            details: Some(attributes_to_struct(intervention.request().details())),
        }),
        provenance: intervention.provenance().map(|provenance| {
            pb::CeremonyInterventionProvenanceState {
                source_intervention_id: provenance.source_intervention_id().as_str().to_owned(),
                source_response_role_id: provenance.source_response_role_id().as_str().to_owned(),
                selected_role_id: provenance.selected_role_id().as_str().to_owned(),
            }
        }),
        responses: intervention
            .responses()
            .iter()
            .map(intervention_response_state_from)
            .collect(),
        created_at: intervention.created_at().to_string(),
        updated_at: intervention.updated_at().to_string(),
        closed_at: intervention
            .closed_at()
            .map(|closed_at| closed_at.to_string())
            .unwrap_or_default(),
    }
}

fn intervention_response_state_from(
    response: &CeremonyInterventionResponse,
) -> pb::CeremonyInterventionResponseState {
    pb::CeremonyInterventionResponseState {
        role_id: response.role_id().as_str().to_owned(),
        content: Some(pb::CeremonyInterventionMessage {
            message: response.content().message().to_owned(),
            details: Some(attributes_to_struct(response.content().details())),
        }),
        evidence_pack: response
            .evidence_pack()
            .map(|pack| serde_json::to_string(pack).unwrap_or_default())
            .unwrap_or_default(),
        responded_at: response.responded_at().to_string(),
    }
}

fn open_intervention_ids(instance: &CeremonyInstance) -> Vec<String> {
    instance
        .interventions()
        .iter()
        .filter(|intervention| intervention.status().is_open())
        .map(|intervention| intervention.id().as_str().to_owned())
        .collect()
}
