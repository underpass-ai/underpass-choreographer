//! What a session did, in the journal's terms.
//!
//! One place turns a thing that happened into a fact that can be
//! sealed, so the shape of a fact is decided once instead of at every
//! call site.
//!
//! # The event id is derived, not generated
//!
//! A retried commit must produce the same fact rather than a second
//! one wearing a new name. Deriving the id from what the fact is about
//! — the session, what happened, and which thing it happened to —
//! makes a retry idempotent by construction, where a fresh identifier
//! each time would turn one approval into two entries in a chain that
//! is supposed to be the record of what happened.

use choreo_core::entities::{AuditFact, CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    AuditActor, AuditActorKind, AuditEventType, CeremonyInterventionId, EventId, GuardName, RoleId,
};
use time::OffsetDateTime;

/// The fact that a human guard was let through.
pub(crate) fn guard_approved(
    instance: &CeremonyInstance,
    guard_name: &GuardName,
    approved_by: &RoleId,
    approved_by_kind: AuditActorKind,
    occurred_at: OffsetDateTime,
) -> Result<AuditFact, DomainError> {
    fact(
        instance,
        AuditEventType::HumanApprovalRecorded,
        &format!("guard:{guard_name}"),
        actor(approved_by, approved_by_kind)?,
        occurred_at,
    )
}

/// The fact that a human guard was left undecided, on purpose.
pub(crate) fn guard_deferred(
    instance: &CeremonyInstance,
    guard_name: &GuardName,
    deferred_by: &RoleId,
    deferred_by_kind: AuditActorKind,
    occurred_at: OffsetDateTime,
) -> Result<AuditFact, DomainError> {
    fact(
        instance,
        AuditEventType::HumanDeferralRecorded,
        &format!("guard:{guard_name}"),
        actor(deferred_by, deferred_by_kind)?,
        occurred_at,
    )
}

/// Who did it, as the journal records actors.
///
/// The kind is carried through from what the caller declared. This
/// engine sees a seat and cannot see what fills it, so the one thing
/// it must not do here is decide.
fn actor(role_id: &RoleId, kind: AuditActorKind) -> Result<AuditActor, DomainError> {
    AuditActor::new(role_id.as_str(), kind, Some(role_id.clone()))
}

fn fact(
    instance: &CeremonyInstance,
    event_type: AuditEventType,
    about: &str,
    actor: AuditActor,
    occurred_at: OffsetDateTime,
) -> Result<AuditFact, DomainError> {
    Ok(AuditFact {
        event_id: EventId::new(format!(
            "{}:{}:{about}",
            instance.id().as_str(),
            event_type.as_str()
        ))?,
        event_type,
        ceremony_id: instance.id().clone(),
        definition_name: instance.definition_name().clone(),
        definition_version: instance.definition_version().clone(),
        occurred_at,
        actor,
        correlation_id: None,
        causation_id: None,
        trace: None,
    })
}

/// The facts a session produces by moving.
///
/// A move is one fact, and reaching an end is another, because they
/// are two different things to have happened: a session that moved and
/// a session that is over are separate claims, and a reader asking
/// "did this finish?" should not have to work it out from the state a
/// move happened to land in.
///
/// Returned together so the caller cannot seal one without the other.
/// Committed apart, a crash between them leaves a session recorded as
/// finished with no move that finished it, or the reverse.
pub(crate) fn transition_applied(
    instance: &CeremonyInstance,
    definition: &CeremonyDefinition,
    applied_by: &RoleId,
    applied_by_kind: AuditActorKind,
    occurred_at: OffsetDateTime,
) -> Result<Vec<AuditFact>, DomainError> {
    let actor = actor(applied_by, applied_by_kind)?;
    // Numbered by how many moves the session has made, so successive
    // transitions are distinct facts while a retry of the same one —
    // which reloads the same session and moves it again from the same
    // place — derives the same id.
    let ordinal = instance.transitions().len();
    let mut facts = vec![fact(
        instance,
        AuditEventType::TransitionApplied,
        &format!("transition:{ordinal}"),
        actor.clone(),
        occurred_at,
    )?];

    // Only one kind of ending is reachable today.
    //
    // `AuditEventType` also has `CeremonyFailed`, and it is tempting to
    // branch on `is_completed` here — but a session reaches a terminal
    // state only by moving into one, and moving into one always stamps
    // it completed. A branch for the other case would be a receipt that
    // can never be issued, which is worse than none: it reads as if the
    // audit distinguishes an abandoned session from a finished one.
    //
    // `a_terminal_session_is_always_a_finished_one` pins that, so the
    // day an ending arrives that is not a completion, this stops being
    // true rather than staying quietly wrong.
    if instance.is_terminal(definition) {
        facts.push(fact(
            instance,
            AuditEventType::CeremonyCompleted,
            &format!("transition:{ordinal}"),
            actor,
            occurred_at,
        )?);
    }
    Ok(facts)
}

/// The fact that a session asked the table for something.
pub(crate) fn intervention_requested(
    instance: &CeremonyInstance,
    intervention_id: &CeremonyInterventionId,
    requested_by: &RoleId,
    requested_by_kind: AuditActorKind,
    occurred_at: OffsetDateTime,
) -> Result<AuditFact, DomainError> {
    fact(
        instance,
        AuditEventType::InterventionRequested,
        &format!("intervention:{intervention_id}"),
        actor(requested_by, requested_by_kind)?,
        occurred_at,
    )
}

/// The fact that a seat answered something the session had asked.
pub(crate) fn intervention_responded(
    instance: &CeremonyInstance,
    intervention_id: &CeremonyInterventionId,
    responded_by: &RoleId,
    responded_by_kind: AuditActorKind,
    occurred_at: OffsetDateTime,
) -> Result<AuditFact, DomainError> {
    // Keyed on the seat as well as the item, because an agenda item
    // put to the whole table is answered by more than one of them and
    // those are separate facts.
    fact(
        instance,
        AuditEventType::InterventionResponded,
        &format!("intervention:{intervention_id}:{responded_by}"),
        actor(responded_by, responded_by_kind)?,
        occurred_at,
    )
}
