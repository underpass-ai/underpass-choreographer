//! The moves only a person makes: proto → application.
//!
//! Guards and agenda items are where a working session stops being a
//! state machine running by itself and becomes something an engineer
//! is in. These conversions carry that across the wire without
//! deciding anything: what a role may do is the engine's business.

use choreo_app::usecases::{
    ApproveCeremonyGuardInput, AssertCeremonyReasonInput, BindCeremonyParticipantsInput,
    CloseCeremonyInterventionInput, CollectCeremonyEvidenceInput, DeferCeremonyGuardInput,
    RequestCeremonyInterventionInput, RespondToCeremonyInterventionInput,
};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    CeremonyEvidenceSourceId, CeremonyGuardDeferralContent, CeremonyId,
    CeremonyInterventionContent, CeremonyInterventionId, CeremonyInterventionKind,
    CeremonyInterventionProvenance, CeremonyInterventionTarget, CeremonyReasonKind,
    CeremonyRecordRef, GuardName, MemoryConfidence, RoleId, Specialty, StepId,
};
use choreo_proto::v1 as pb;
use uuid::Uuid;

use super::actor_kind::actor_kind_from_proto;
use super::attributes::attributes_from_struct;

pub fn approve_ceremony_guard_input_from_proto(
    request: pb::ApproveCeremonyGuardRequest,
) -> Result<ApproveCeremonyGuardInput, DomainError> {
    Ok(ApproveCeremonyGuardInput::new(
        CeremonyId::new(request.ceremony_id)?,
        GuardName::new(request.guard_name)?,
        RoleId::new(request.role_id)?,
        actor_kind_from_proto(&request.role_kind, "role_kind")?,
    ))
}

/// Something a session produced, from the flat shape on the wire.
///
/// Only the field the kind names is read. A caller that fills the rest
/// is not corrected: the discriminator is what the message means, and
/// treating stray fields as an error would make a tolerant client an
/// error case for no gain.
pub fn ceremony_record_ref_from_proto(
    state: Option<pb::CeremonyRecordRefState>,
    field: &'static str,
) -> Result<CeremonyRecordRef, DomainError> {
    let state = state.ok_or(DomainError::EmptyField { field })?;
    Ok(match state.kind.as_str() {
        "step" => CeremonyRecordRef::step(StepId::new(state.step_id)?),
        "agenda_item" => {
            CeremonyRecordRef::agenda_item(CeremonyInterventionId::new(state.agenda_item)?)
        }
        "contribution" => CeremonyRecordRef::contribution(
            CeremonyInterventionId::new(state.agenda_item)?,
            state.ordinal,
        ),
        "guard_decision" => CeremonyRecordRef::guard_decision(GuardName::new(state.guard_name)?),
        "transition" => CeremonyRecordRef::transition(state.ordinal),
        _ => {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_record_ref.kind",
            })
        }
    })
}

fn reason_kind_from_proto(raw: &str) -> Result<CeremonyReasonKind, DomainError> {
    Ok(match raw {
        "chosen_because" => CeremonyReasonKind::ChosenBecause,
        "achieved_by" => CeremonyReasonKind::AchievedBy,
        "follows_from" => CeremonyReasonKind::FollowsFrom,
        "satisfies_constraint" => CeremonyReasonKind::SatisfiesConstraint,
        "violates_constraint" => CeremonyReasonKind::ViolatesConstraint,
        "supersedes" => CeremonyReasonKind::Supersedes,
        "contradicts" => CeremonyReasonKind::Contradicts,
        // Not an oversight: it states the shape of the session rather
        // than anyone's judgement, and only the engine asserts it.
        "answers" => {
            return Err(DomainError::InvariantViolated {
                reason: "only the engine may assert that one thing answers another",
            })
        }
        _ => {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_reason.kind",
            })
        }
    })
}

fn confidence_from_proto(raw: &str) -> Result<MemoryConfidence, DomainError> {
    Ok(match raw {
        "high" => MemoryConfidence::High,
        "medium" => MemoryConfidence::Medium,
        "low" => MemoryConfidence::Low,
        _ => {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_reason.confidence",
            })
        }
    })
}

pub fn assert_ceremony_reason_input_from_proto(
    request: pb::AssertCeremonyReasonRequest,
) -> Result<AssertCeremonyReasonInput, DomainError> {
    Ok(AssertCeremonyReasonInput::new(
        CeremonyId::new(request.ceremony_id)?,
        RoleId::new(request.role_id)?,
        ceremony_record_ref_from_proto(request.from, "ceremony_reason.from")?,
        ceremony_record_ref_from_proto(request.to, "ceremony_reason.to")?,
        reason_kind_from_proto(&request.kind)?,
        request.why,
        confidence_from_proto(&request.confidence)?,
    ))
}

pub fn defer_ceremony_guard_input_from_proto(
    request: pb::DeferCeremonyGuardRequest,
) -> Result<DeferCeremonyGuardInput, DomainError> {
    Ok(DeferCeremonyGuardInput::new(
        CeremonyId::new(request.ceremony_id)?,
        GuardName::new(request.guard_name)?,
        CeremonyGuardDeferralContent::new(
            request.statement,
            request.reason,
            request.reconsider_when,
        )?,
        RoleId::new(request.role_id)?,
        actor_kind_from_proto(&request.role_kind, "role_kind")?,
    ))
}

pub fn request_ceremony_intervention_input_from_proto(
    request: pb::RequestCeremonyInterventionRequest,
) -> Result<RequestCeremonyInterventionInput, DomainError> {
    let intervention_id = if request.intervention_id.trim().is_empty() {
        CeremonyInterventionId::new(Uuid::new_v4().to_string())?
    } else {
        CeremonyInterventionId::new(request.intervention_id)?
    };
    // No target roles means the item is put to the table rather than
    // to nobody: an unanswerable agenda item is not a useful thing to
    // be able to express.
    let target = if request.target_role_ids.is_empty() {
        CeremonyInterventionTarget::table()
    } else {
        CeremonyInterventionTarget::roles(
            request
                .target_role_ids
                .into_iter()
                .map(RoleId::new)
                .collect::<Result<Vec<_>, _>>()?,
        )?
    };
    let content = CeremonyInterventionContent::new(
        request.message,
        attributes_from_struct(request.details)?,
    )?;

    let mut input = RequestCeremonyInterventionInput::new(
        CeremonyId::new(request.ceremony_id)?,
        intervention_id,
        RoleId::new(request.role_id)?,
        actor_kind_from_proto(&request.role_kind, "role_kind")?,
        intervention_kind_from_proto(&request.kind)?,
        target,
        content,
    );
    if let Some(provenance) = request.provenance {
        input = input.with_provenance(provenance_from_proto(provenance)?);
    }
    Ok(input)
}

pub fn respond_to_ceremony_intervention_input_from_proto(
    request: pb::RespondToCeremonyInterventionRequest,
) -> Result<RespondToCeremonyInterventionInput, DomainError> {
    Ok(RespondToCeremonyInterventionInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyInterventionId::new(request.intervention_id)?,
        RoleId::new(request.role_id)?,
        actor_kind_from_proto(&request.role_kind, "role_kind")?,
        CeremonyInterventionContent::new(
            request.message,
            attributes_from_struct(request.details)?,
        )?,
    ))
}

pub fn close_ceremony_intervention_input_from_proto(
    request: pb::CloseCeremonyInterventionRequest,
) -> Result<CloseCeremonyInterventionInput, DomainError> {
    Ok(CloseCeremonyInterventionInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyInterventionId::new(request.intervention_id)?,
        RoleId::new(request.role_id)?,
        actor_kind_from_proto(&request.role_kind, "role_kind")?,
    ))
}

pub fn collect_ceremony_evidence_input_from_proto(
    request: pb::CollectCeremonyEvidenceRequest,
) -> Result<CollectCeremonyEvidenceInput, DomainError> {
    Ok(CollectCeremonyEvidenceInput::new(
        CeremonyId::new(request.ceremony_id)?,
        CeremonyInterventionId::new(request.intervention_id)?,
        RoleId::new(request.role_id)?,
        actor_kind_from_proto(&request.role_kind, "role_kind")?,
        CeremonyEvidenceSourceId::new(request.source_id)?,
        CeremonyInterventionContent::new(request.query, attributes_from_struct(request.details)?)?,
    ))
}

/// The same three words the in-process surface accepts. Rendered as a
/// string rather than a proto enum so both distributions spell the
/// kind identically — an enum here would make the wire say
/// `INTERVENTION_KIND_OPINION` where the other says `opinion`.
fn intervention_kind_from_proto(raw: &str) -> Result<CeremonyInterventionKind, DomainError> {
    match raw {
        "opinion" => Ok(CeremonyInterventionKind::Opinion),
        "investigation" => Ok(CeremonyInterventionKind::Investigation),
        "action" => Ok(CeremonyInterventionKind::Action),
        _ => Err(DomainError::InvariantViolated {
            reason: "intervention kind must be one of: opinion, investigation, action",
        }),
    }
}

fn provenance_from_proto(
    provenance: pb::CeremonyInterventionProvenanceState,
) -> Result<CeremonyInterventionProvenance, DomainError> {
    Ok(CeremonyInterventionProvenance::selected_from(
        CeremonyInterventionId::new(provenance.source_intervention_id)?,
        RoleId::new(provenance.source_response_role_id)?,
        RoleId::new(provenance.selected_role_id)?,
    ))
}

/// Seating for one session: role id to the specialty its work is put
/// to. Empty is refused by the input itself, so a caller who sent
/// nothing hears that rather than "done".
pub fn bind_ceremony_participants_input_from_proto(
    request: pb::BindCeremonyParticipantsRequest,
) -> Result<BindCeremonyParticipantsInput, DomainError> {
    let seating = request
        .seating
        .into_iter()
        .map(|(role_id, specialty)| Ok((RoleId::new(role_id)?, Specialty::new(specialty)?)))
        .collect::<Result<Vec<_>, DomainError>>()?;
    BindCeremonyParticipantsInput::new(CeremonyId::new(request.ceremony_id)?, seating)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intervention_request() -> pb::RequestCeremonyInterventionRequest {
        pb::RequestCeremonyInterventionRequest {
            ceremony_id: "session-1".to_owned(),
            intervention_id: String::new(),
            role_id: "FACILITATOR".to_owned(),
            role_kind: "human".to_owned(),
            kind: "investigation".to_owned(),
            target_role_ids: Vec::new(),
            message: "Which table holds the queued messages?".to_owned(),
            details: None,
            provenance: None,
        }
    }

    #[test]
    fn an_item_with_no_named_roles_is_put_to_the_table() {
        let input = request_ceremony_intervention_input_from_proto(intervention_request()).unwrap();

        assert_eq!(input.target(), &CeremonyInterventionTarget::table());
        // An id is minted rather than left empty, so the item can be
        // answered without a round-trip to discover its name.
        assert!(!input.intervention_id().as_str().is_empty());
    }

    #[test]
    fn named_roles_are_who_the_item_is_put_to() {
        let request = pb::RequestCeremonyInterventionRequest {
            target_role_ids: vec!["RISK_REVIEWER".to_owned(), "SYNTHESIZER".to_owned()],
            ..intervention_request()
        };

        let input = request_ceremony_intervention_input_from_proto(request).unwrap();

        assert_eq!(
            input.target(),
            &CeremonyInterventionTarget::roles(vec![
                RoleId::new("RISK_REVIEWER").unwrap(),
                RoleId::new("SYNTHESIZER").unwrap(),
            ])
            .unwrap()
        );
    }

    #[test]
    fn an_unknown_kind_is_refused_rather_than_guessed() {
        let request = pb::RequestCeremonyInterventionRequest {
            kind: "escalation".to_owned(),
            ..intervention_request()
        };

        let error = request_ceremony_intervention_input_from_proto(request).unwrap_err();

        assert!(matches!(error, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn a_deferral_carries_what_was_decided_and_what_would_reopen_it() {
        let input = defer_ceremony_guard_input_from_proto(pb::DeferCeremonyGuardRequest {
            role_kind: "human".to_owned(),
            role_id: "facilitator".to_owned(),
            ceremony_id: "session-1".to_owned(),
            guard_name: "budget_approved".to_owned(),
            statement: "Not approving today.".to_owned(),
            reason: "The cost figure is a guess.".to_owned(),
            reconsider_when: vec!["a measured figure exists".to_owned()],
        })
        .unwrap();

        assert_eq!(input.guard_name().as_str(), "budget_approved");
        assert_eq!(input.content().reconsider_when().len(), 1);
    }
}
