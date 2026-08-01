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

use choreo_core::entities::{AuditFact, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    AuditActor, AuditActorKind, AuditEventType, EventId, GuardName, RoleId,
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
