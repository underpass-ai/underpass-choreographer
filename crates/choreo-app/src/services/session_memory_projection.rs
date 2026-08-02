//! What a session leaves behind, in memory's terms.
//!
//! The seam between two contracts that were designed apart: a session
//! knows records and reasons, memory knows entries and relations. Both
//! splits are the same split — the what and the why — so the mapping
//! is small, and where the two models genuinely differ the difference
//! is decided here rather than smoothed over in five call sites.
//!
//! # Not everything a session produces is worth remembering
//!
//! A step running is machinery, and an agenda item is a question.
//! Neither is a decision, an observation, a constraint or an outcome,
//! and memory has no fifth kind for "everything else" — deliberately,
//! because that is the kind transcripts would arrive under.
//!
//! That has a consequence worth stating rather than discovering: **a
//! reason with an end that was not remembered is not written**. An
//! edge into nothing claims an explanation exists and gives no way to
//! reach it. It is also why the one reason the engine observes on its
//! own — that a contribution answers its agenda item — never reaches
//! memory: the item is not an entry. Nothing is lost by it, since the
//! contribution carries the item as its own axis and can be found
//! along it.

use choreo_core::entities::{CeremonyDefinition, CeremonyInstance};
use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    Attributes, CeremonyGuardApproval, CeremonyGuardDeferral, CeremonyInterventionKind,
    CeremonyReason, CeremonyReasonKind, CeremonyRecordRef, MemoryDimension, MemoryEntry,
    MemoryEntryId, MemoryEntryKind, MemoryEvidence, MemoryProvenance, MemoryRelation,
    MemoryRelationKind, RoleId,
};
use time::OffsetDateTime;

/// The name a session record is remembered under.
///
/// Deterministic, so a reason asserted an hour later still points at
/// the entry written when the thing happened. Readable, because these
/// end up in somebody else's graph and a person may have to look.
pub(super) fn entry_id(record: &CeremonyRecordRef) -> Result<MemoryEntryId, DomainError> {
    let raw = match record {
        CeremonyRecordRef::Step { step_id } => format!("step:{step_id}"),
        CeremonyRecordRef::AgendaItem { agenda_item } => format!("agenda:{agenda_item}"),
        CeremonyRecordRef::Contribution {
            agenda_item,
            ordinal,
        } => format!("agenda:{agenda_item}:contribution:{ordinal}"),
        CeremonyRecordRef::GuardDecision { guard_name } => format!("guard:{guard_name}"),
        CeremonyRecordRef::Transition { ordinal } => format!("transition:{ordinal}"),
    };
    MemoryEntryId::new(raw)
}

/// How a session's kind of reason is said in memory's vocabulary.
///
/// One to one, because both were written from the same idea. Kept as
/// a mapping rather than a shared type so neither contract has to move
/// when the other learns a new kind.
pub(super) const fn relation_kind(kind: CeremonyReasonKind) -> MemoryRelationKind {
    match kind {
        CeremonyReasonKind::Answers => MemoryRelationKind::Answers,
        CeremonyReasonKind::Authorizes => MemoryRelationKind::Authorizes,
        CeremonyReasonKind::ChosenBecause => MemoryRelationKind::ChosenBecause,
        CeremonyReasonKind::AchievedBy => MemoryRelationKind::AchievedBy,
        CeremonyReasonKind::FollowsFrom => MemoryRelationKind::FollowsFrom,
        CeremonyReasonKind::SatisfiesConstraint => MemoryRelationKind::SatisfiesConstraint,
        CeremonyReasonKind::ViolatesConstraint => MemoryRelationKind::ViolatesConstraint,
        CeremonyReasonKind::Supersedes => MemoryRelationKind::Supersedes,
        CeremonyReasonKind::Contradicts => MemoryRelationKind::Contradicts,
    }
}

/// The entry for one contribution to an agenda item.
///
/// Whether it is a decision or an observation is read off what was
/// asked, not guessed from the wording: an investigation comes back
/// with what was found, and an opinion or an action comes back with
/// something settled.
pub(super) fn contribution_entry(
    instance: &CeremonyInstance,
    record: &CeremonyRecordRef,
) -> Result<Option<MemoryEntry>, DomainError> {
    let CeremonyRecordRef::Contribution {
        agenda_item,
        ordinal,
    } = record
    else {
        return Ok(None);
    };
    let Some(item) = instance.intervention(agenda_item) else {
        return Ok(None);
    };
    let Some(response) = item.responses().get(*ordinal as usize) else {
        return Ok(None);
    };

    let kind = match item.kind() {
        CeremonyInterventionKind::Investigation => MemoryEntryKind::Observation,
        CeremonyInterventionKind::Opinion | CeremonyInterventionKind::Action => {
            MemoryEntryKind::Decision
        }
    };

    let entry = MemoryEntry::new(
        entry_id(record)?,
        kind,
        response.content().message(),
        Some(MemoryDimension::new(format!("agenda:{agenda_item}"))?),
        provenance(instance, Some(response.role_id()), response.responded_at()),
        Attributes::empty(),
    )?
    .with_evidence(evidence_references(response)?);
    Ok(Some(entry))
}

/// The entry for a human decision on a guard.
///
/// An approval settles something, so it is a decision. A deferral is
/// a constraint: it says this may not proceed yet, why, and what would
/// change that — which is what a later session needs in order not to
/// walk into the same wall.
pub(super) fn guard_entry(
    instance: &CeremonyInstance,
    record: &CeremonyRecordRef,
) -> Result<Option<MemoryEntry>, DomainError> {
    let CeremonyRecordRef::GuardDecision { guard_name } = record else {
        return Ok(None);
    };

    if let Some(approval) = instance
        .guard_approvals()
        .iter()
        .find(|approval| approval.guard_name() == guard_name)
    {
        return Ok(Some(approval_entry(instance, record, approval)?));
    }
    if let Some(deferral) = instance
        .guard_deferrals()
        .iter()
        .find(|deferral| deferral.guard_name() == guard_name)
    {
        return Ok(Some(deferral_entry(instance, record, deferral)?));
    }
    Ok(None)
}

fn approval_entry(
    instance: &CeremonyInstance,
    record: &CeremonyRecordRef,
    approval: &CeremonyGuardApproval,
) -> Result<MemoryEntry, DomainError> {
    MemoryEntry::new(
        entry_id(record)?,
        MemoryEntryKind::Decision,
        format!("`{}` was approved", approval.guard_name()),
        None,
        provenance(
            instance,
            Some(approval.approved_by()),
            approval.approved_at(),
        ),
        Attributes::empty(),
    )
}

fn deferral_entry(
    instance: &CeremonyInstance,
    record: &CeremonyRecordRef,
    deferral: &CeremonyGuardDeferral,
) -> Result<MemoryEntry, DomainError> {
    // What would make it worth asking again travels with it. A
    // deferral without that is a dead end; with it, it is a condition
    // a later session can check.
    let summary = format!(
        "`{}` was not decided: {} — worth revisiting when {}",
        deferral.guard_name(),
        deferral.content().reason(),
        deferral.content().reconsider_when().join("; ")
    );
    MemoryEntry::new(
        entry_id(record)?,
        MemoryEntryKind::Constraint,
        summary,
        None,
        provenance(
            instance,
            Some(deferral.deferred_by()),
            deferral.deferred_at(),
        ),
        Attributes::empty(),
    )
}

/// The entry for the move that ended a session.
///
/// Only the ending. The moves along the way are how a session got
/// somewhere, and remembering each of them would fill memory with
/// mechanics — the thing that makes navigating it slower rather than
/// possible.
pub(super) fn ending_entry(
    instance: &CeremonyInstance,
    definition: &CeremonyDefinition,
) -> Result<Option<(CeremonyRecordRef, MemoryEntry)>, DomainError> {
    if !instance.is_terminal(definition) {
        return Ok(None);
    }
    let Some(ordinal) = u32::try_from(instance.transitions().len())
        .ok()
        .filter(|n| *n > 0)
    else {
        return Ok(None);
    };
    let Some(ending) = instance.transitions().last() else {
        return Ok(None);
    };

    // Reaching an end is not the same as arriving at the intended one,
    // and a later session weighing this as a precedent would need to
    // know which happened — but there is only one kind of ending to
    // tell it about. A session reaches a terminal state only by moving
    // into one, and moving into one always stamps it completed, so the
    // branch that used to say "stopped without finishing" could never
    // run. It read as though memory distinguished an abandoned session
    // from a finished one; it does not, and saying so was the lie.
    //
    // `a_terminal_session_is_always_a_finished_one` is where this stops
    // being true. Whoever makes a session abandonable comes back here.
    let summary = format!("the session finished in `{}`", ending.to_state());

    let record = CeremonyRecordRef::transition(ordinal);
    let entry = MemoryEntry::new(
        entry_id(&record)?,
        MemoryEntryKind::Outcome,
        summary,
        None,
        provenance(instance, ending.applied_by(), ending.applied_at()),
        Attributes::empty(),
    )?;
    Ok(Some((record, entry)))
}

/// A reason, if both ends of it were remembered.
///
/// An edge into something that was never written claims an
/// explanation exists and gives no way to reach it, which is worse
/// than admitting there is none.
pub(super) fn relation(
    reason: &CeremonyReason,
    remembered: &dyn Fn(&CeremonyRecordRef) -> bool,
) -> Result<Option<MemoryRelation>, DomainError> {
    if !remembered(reason.from()) || !remembered(reason.to()) {
        return Ok(None);
    }
    Ok(Some(MemoryRelation::new(
        entry_id(reason.from())?,
        entry_id(reason.to())?,
        relation_kind(reason.kind()),
        reason.why(),
        reason.confidence(),
    )?))
}

fn provenance(
    instance: &CeremonyInstance,
    role_id: Option<&RoleId>,
    observed_at: OffsetDateTime,
) -> MemoryProvenance {
    MemoryProvenance::new(instance.id().clone(), role_id.cloned(), observed_at)
}

/// What backed a contribution, as references rather than as content.
///
/// The title says what it was, the source says where it came from, the
/// item id says which one — enough to go and look. The narrative is
/// left out: it is the raw material the reference points at, and the
/// line between the two is what keeps memory navigable.
fn evidence_references(
    response: &choreo_core::value_objects::CeremonyInterventionResponse,
) -> Result<Vec<MemoryEvidence>, DomainError> {
    let Some(pack) = response.evidence_pack() else {
        return Ok(Vec::new());
    };
    pack.bundle()
        .items()
        .iter()
        .map(|item| {
            MemoryEvidence::new(
                item.title(),
                Some(pack.source_id().to_string()),
                Attributes::new(
                    [
                        ("item_id".to_owned(), serde_json::json!(item.item_id())),
                        ("kind".to_owned(), serde_json::json!(item.kind())),
                    ]
                    .into_iter()
                    .collect(),
                )?,
            )
        })
        .collect()
}
