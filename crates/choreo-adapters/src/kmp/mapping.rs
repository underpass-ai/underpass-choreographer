//! Turning what a session remembers into what a kernel stores, and back.
//!
//! The two models very nearly line up, and where they do not the
//! difference is recorded here rather than smoothed over.
//!
//! A session entry carries its own axis, who saw it and when; a kernel
//! entry carries text, a kind, and coordinates on named dimensions.
//! So provenance is written **as dimensions** — the session it belongs
//! to, the ceremony that produced it, the role that saw it, the strand
//! it runs along — which is both what the kernel wants and what makes
//! a memory navigable afterwards. Provenance smuggled through an
//! opaque metadata bag would come back as nothing, because the
//! kernel's read surface does not return one.

use std::collections::BTreeMap;

use choreo_core::error::DomainError;
use choreo_core::value_objects::{
    Attributes, CeremonyId, MemoryDimension, MemoryEntry, MemoryEntryKind, MemoryEvidence,
    MemoryMoment, MemoryProvenance, MemoryScope, RoleId,
};
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// The dimension a session's own memory runs along.
const DIMENSION_SESSION: &str = "session";
/// The dimension carrying an entry's own axis within the session.
const DIMENSION_STRAND: &str = "strand";
/// The dimension naming the ceremony an entry came out of.
const DIMENSION_CEREMONY: &str = "ceremony";
/// The dimension naming the role that saw it.
const DIMENSION_ROLE: &str = "role";

/// How many entries to ask for in one page.
///
/// The kernel returns a single entry when not told otherwise, so this
/// is not a limit being imposed but one being lifted.
pub(super) const PAGE_SIZE: u64 = 500;

/// Reading everything is reading as of the end of time.
///
/// The kernel's temporal read wants a cursor and has no "everything"
/// form, and inventing one here would mean a second code path that
/// could drift from the first. Recall is the same journey with the
/// destination set past any session that will ever be written.
pub(super) fn end_of_time() -> MemoryMoment {
    MemoryMoment::at(time::macros::datetime!(9999-12-31 00:00:00 UTC))
}

/// What one page of a temporal read yielded.
///
/// Entries keep the reference they came back under, because paging
/// through a temporal read can show the same entry twice and the
/// reference is the only thing that says so.
#[derive(Debug, Default)]
pub(super) struct RecalledPage {
    pub(super) entries: Vec<(String, MemoryEntry)>,
    pub(super) next_cursor: Option<String>,
    /// Entries the kernel returned that this engine cannot represent.
    pub(super) unreadable: usize,
}

/// The arguments for writing `entries` about `scope`.
pub(super) fn ingest_arguments(
    scope: &MemoryScope,
    entries: &[MemoryEntry],
    idempotency_key: &str,
) -> Result<Value, DomainError> {
    let session = qualify(DIMENSION_SESSION, scope.as_str());
    let mut dimensions: BTreeMap<String, Value> = BTreeMap::new();
    declare(&mut dimensions, &session, DIMENSION_SESSION);

    let mut kernel_entries = Vec::with_capacity(entries.len());
    let mut kernel_evidence = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        let provenance = entry.provenance();
        let observed_at = timestamp(provenance.observed_at())?;
        let sequence = index as u64 + 1;

        let mut coordinates = vec![coordinate(
            DIMENSION_SESSION,
            &session,
            sequence,
            &observed_at,
        )];

        let ceremony = qualify(DIMENSION_CEREMONY, provenance.ceremony_id().as_str());
        declare(&mut dimensions, &ceremony, DIMENSION_CEREMONY);
        coordinates.push(coordinate(
            DIMENSION_CEREMONY,
            &ceremony,
            sequence,
            &observed_at,
        ));

        if let Some(role) = provenance.role_id() {
            let role = qualify(DIMENSION_ROLE, role.as_str());
            declare(&mut dimensions, &role, DIMENSION_ROLE);
            coordinates.push(coordinate(DIMENSION_ROLE, &role, sequence, &observed_at));
        }

        if let Some(strand) = entry.dimension() {
            let strand = qualify(DIMENSION_STRAND, strand.as_str());
            declare(&mut dimensions, &strand, DIMENSION_STRAND);
            coordinates.push(coordinate(
                DIMENSION_STRAND,
                &strand,
                sequence,
                &observed_at,
            ));
        }

        let entry_ref = entry_ref(scope, idempotency_key, index);
        kernel_entries.push(json!({
            "id": entry_ref,
            "kind": entry.kind().as_label(),
            "text": entry.summary(),
            "coordinates": coordinates,
            "metadata": flatten(entry.detail()),
        }));

        for (ordinal, evidence) in entry.evidence().iter().enumerate() {
            let mut item = Map::new();
            item.insert(
                "id".to_owned(),
                json!(evidence_ref(scope, idempotency_key, index, ordinal)),
            );
            item.insert("text".to_owned(), json!(evidence.label()));
            item.insert("supports".to_owned(), json!([entry_ref]));
            item.insert("metadata".to_owned(), json!(flatten(evidence.detail())));
            if let Some(source) = evidence.source_id() {
                item.insert("source".to_owned(), json!(source));
            }
            kernel_evidence.push(Value::Object(item));
        }
    }

    let observed_at = entries
        .first()
        .map(|entry| timestamp(entry.provenance().observed_at()))
        .transpose()?
        .unwrap_or_default();

    Ok(json!({
        "about": scope.as_str(),
        "idempotency_key": idempotency_key,
        "memory": {
            "dimensions": dimensions.into_values().collect::<Vec<_>>(),
            "entries": kernel_entries,
            "evidence": kernel_evidence,
        },
        "provenance": {
            "source_kind": "agent",
            "source_agent": "choreographer",
            "observed_at": observed_at,
        },
    }))
}

/// The arguments for reading `scope` as it stood at `moment`.
pub(super) fn goto_arguments(
    scope: &MemoryScope,
    moment: MemoryMoment,
    cursor: Option<&str>,
) -> Result<Value, DomainError> {
    let at = match cursor {
        Some(reference) => json!({ "ref": reference }),
        None => json!({ "time": timestamp(moment.instant())? }),
    };
    Ok(json!({
        "about": scope.as_str(),
        "at": at,
        "include": { "evidence": true, "relations": true },
        "limit": { "entries": PAGE_SIZE },
    }))
}

/// Read one page of a temporal answer back into session memory.
///
/// Entries this engine cannot represent — a kind it does not model, a
/// summary longer than one may be, provenance it cannot attribute —
/// are left out and counted rather than repaired. A scope may hold
/// memory another writer put there, and inventing a kind or an author
/// for it would be worse than admitting it is not ours to read.
pub(super) fn read_page(scope: &MemoryScope, document: &Value) -> RecalledPage {
    let attached = evidence_by_entry(document);
    let mut page = RecalledPage::default();

    for value in document
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match read_entry(scope, value, &attached) {
            Some(found) => page.entries.push(found),
            None => page.unreadable += 1,
        }
    }

    if document
        .get("page")
        .and_then(|page| page.get("has_more"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        page.next_cursor = document
            .get("page")
            .and_then(|page| page.get("next_cursor"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }

    page
}

fn read_entry(
    scope: &MemoryScope,
    value: &Value,
    attached: &BTreeMap<String, Vec<MemoryEvidence>>,
) -> Option<(String, MemoryEntry)> {
    let reference = value.get("ref").and_then(Value::as_str)?;
    let kind = kind_of(value.get("kind").and_then(Value::as_str)?)?;
    let summary = value.get("text").and_then(Value::as_str)?;

    let mut ceremony = None;
    let mut role = None;
    let mut strand = None;
    let mut observed_at = None;

    for coordinate in value
        .get("coordinates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let dimension = coordinate.get("dimension").and_then(Value::as_str);
        let scope_id = coordinate
            .get("scope_id")
            .and_then(Value::as_str)
            .map(|raw| declared_id(scope, raw));

        match (dimension, scope_id) {
            (Some(DIMENSION_CEREMONY), Some(id)) => ceremony = unqualify(DIMENSION_CEREMONY, id),
            (Some(DIMENSION_ROLE), Some(id)) => role = unqualify(DIMENSION_ROLE, id),
            (Some(DIMENSION_STRAND), Some(id)) => strand = unqualify(DIMENSION_STRAND, id),
            _ => {}
        }
        if observed_at.is_none() {
            observed_at = coordinate
                .get("occurred_at")
                .and_then(Value::as_str)
                .and_then(|raw| OffsetDateTime::parse(raw, &Rfc3339).ok());
        }
    }

    let provenance = MemoryProvenance::new(
        CeremonyId::new(ceremony?).ok()?,
        role.and_then(|role| RoleId::new(role).ok()),
        observed_at?,
    );
    let dimension = strand.and_then(|strand| MemoryDimension::new(strand).ok());

    let entry = MemoryEntry::new(kind, summary, dimension, provenance, Attributes::empty()).ok()?;
    let entry = match attached.get(reference) {
        Some(evidence) => entry.with_evidence(evidence.iter().cloned()),
        None => entry,
    };
    Some((reference.to_owned(), entry))
}

/// Which evidence backs which entry.
///
/// The link lives in the proof path rather than beside the entry, so
/// it is read from there: every relation that says one thing supports
/// another, with the supporting text the kernel carries on it.
fn evidence_by_entry(document: &Value) -> BTreeMap<String, Vec<MemoryEvidence>> {
    let proof = document.get("proof");
    let texts: BTreeMap<&str, &str> = proof
        .and_then(|proof| proof.get("evidence"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item.get("source").and_then(Value::as_str)?,
                item.get("text").and_then(Value::as_str)?,
            ))
        })
        .collect();

    let mut attached: BTreeMap<String, Vec<MemoryEvidence>> = BTreeMap::new();
    for relation in proof
        .and_then(|proof| proof.get("path"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if relation.get("rel").and_then(Value::as_str) != Some("supports") {
            continue;
        }
        let Some(supported) = relation.get("to").and_then(Value::as_str) else {
            continue;
        };
        let label = relation
            .get("evidence")
            .and_then(Value::as_str)
            .or_else(|| {
                relation
                    .get("from")
                    .and_then(Value::as_str)
                    .and_then(|from| texts.get(from).copied())
            });
        let Some(label) = label else { continue };
        let Ok(evidence) = MemoryEvidence::new(label, None, Attributes::empty()) else {
            continue;
        };
        attached
            .entry(supported.to_owned())
            .or_default()
            .push(evidence);
    }
    attached
}

/// Whether a refusal means "nothing is written there".
///
/// The kernel says so in words rather than in a code, so this reads
/// the words. Should that wording ever change, an unwritten scope
/// starts surfacing as an error instead of as emptiness — which is
/// the safe direction for a mistake to fall, and the reason this is
/// matched narrowly rather than by treating every refusal as empty.
pub(super) fn means_nothing_is_written(refusal: &str) -> bool {
    refusal.contains("not found")
}

/// Whether a refusal means "this write was already made".
///
/// The kernel does not answer a replay with a quiet success. Its
/// translation of a write depends on what is already stored, so the
/// second attempt never hashes to the first and it refuses, leaving
/// its state as it was.
///
/// It refuses in the same words when a caller reuses one key for
/// different memory, and gives no way to tell the two apart. Both are
/// reported upward as already remembered, which is true of what the
/// kernel now holds; reusing a key for a different write is a caller
/// error the port already forbids, and one that leaves no trace here.
pub(super) fn means_already_remembered(refusal: &str) -> bool {
    refusal.contains("idempotency key") && refusal.contains("already accepted")
}

fn kind_of(label: &str) -> Option<MemoryEntryKind> {
    match label {
        "decision" => Some(MemoryEntryKind::Decision),
        "observation" => Some(MemoryEntryKind::Observation),
        "constraint" => Some(MemoryEntryKind::Constraint),
        "outcome" => Some(MemoryEntryKind::Outcome),
        _ => None,
    }
}

fn declare(dimensions: &mut BTreeMap<String, Value>, id: &str, kind: &str) {
    dimensions
        .entry(id.to_owned())
        .or_insert_with(|| json!({ "id": id, "kind": kind }));
}

fn coordinate(dimension: &str, scope_id: &str, sequence: u64, occurred_at: &str) -> Value {
    json!({
        "dimension": dimension,
        "scope_id": scope_id,
        "sequence": sequence,
        "occurred_at": occurred_at,
    })
}

/// A dimension id says what kind of thing it names.
///
/// Every id is qualified, the session's included, and that is the
/// point rather than tidiness. A scope is often named after the very
/// ceremony writing to it, so an unqualified session id and a
/// ceremony id would be the same string describing two different
/// axes. The kernel happens to tolerate that today; a boundary held
/// together by the other side's tolerance is one that breaks on their
/// release, not ours.
fn qualify(kind: &str, value: &str) -> String {
    format!("{kind}:{value}")
}

fn unqualify(kind: &str, id: &str) -> Option<String> {
    id.strip_prefix(&format!("{kind}:")).map(ToOwned::to_owned)
}

/// The id a dimension was declared under.
///
/// The kernel namespaces a scope id by the memory it belongs to on the
/// way back, so the name that went in is recovered by removing what
/// the kernel added.
fn declared_id<'a>(scope: &MemoryScope, returned: &'a str) -> &'a str {
    let prefix = format!("about:{}:dimension:", scope.as_str());
    returned.strip_prefix(&prefix).unwrap_or(returned)
}

/// The kernel's metadata is strings; an attribute may be any document.
fn flatten(attributes: &Attributes) -> BTreeMap<String, String> {
    attributes
        .as_map()
        .iter()
        .map(|(key, value)| {
            let flattened = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            (key.clone(), flattened)
        })
        .collect()
}

fn timestamp(instant: OffsetDateTime) -> Result<String, DomainError> {
    instant
        .format(&Rfc3339)
        .map_err(|_| DomainError::OutOfRange {
            field: "memory.observed_at",
            value: instant.unix_timestamp() as f64,
            min: 0.0,
            max: f64::MAX,
        })
}

fn entry_ref(scope: &MemoryScope, idempotency_key: &str, index: usize) -> String {
    format!("entry:{}:{idempotency_key}:{index}", scope.as_str())
}

fn evidence_ref(
    scope: &MemoryScope,
    idempotency_key: &str,
    index: usize,
    ordinal: usize,
) -> String {
    format!(
        "evidence:{}:{idempotency_key}:{index}:{ordinal}",
        scope.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> MemoryScope {
        MemoryScope::new("ceremony:seven").expect("a valid scope")
    }

    #[test]
    fn a_qualified_id_gives_its_value_back() {
        assert_eq!(qualify(DIMENSION_ROLE, "responder"), "role:responder");
        assert_eq!(
            unqualify(DIMENSION_ROLE, "role:responder").as_deref(),
            Some("responder")
        );
    }

    /// A value that carries the separator itself must survive whole —
    /// a scope named after a ceremony is exactly that case.
    #[test]
    fn qualifying_is_reversible_even_when_the_value_has_colons() {
        let qualified = qualify(DIMENSION_SESSION, "ceremony:seven");

        assert_eq!(qualified, "session:ceremony:seven");
        assert_eq!(
            unqualify(DIMENSION_SESSION, &qualified).as_deref(),
            Some("ceremony:seven")
        );
    }

    /// An id under another kind is not this kind's to read.
    #[test]
    fn unqualifying_the_wrong_kind_yields_nothing() {
        assert_eq!(unqualify(DIMENSION_CEREMONY, "role:responder"), None);
    }

    #[test]
    fn a_namespaced_scope_id_loses_what_the_kernel_added() {
        assert_eq!(
            declared_id(&scope(), "about:ceremony:seven:dimension:role:responder"),
            "role:responder"
        );
    }

    /// A kernel that stops namespacing, or one that namespaces
    /// differently, leaves the id alone rather than mangled: better a
    /// coordinate that fails to match than one that matches wrongly.
    #[test]
    fn an_unnamespaced_scope_id_is_left_as_it_is() {
        assert_eq!(declared_id(&scope(), "role:responder"), "role:responder");
    }

    #[test]
    fn only_the_four_kinds_are_read_back() {
        assert_eq!(kind_of("decision"), Some(MemoryEntryKind::Decision));
        assert_eq!(kind_of("outcome"), Some(MemoryEntryKind::Outcome));
        assert_eq!(
            kind_of("claim"),
            None,
            "a kind this engine does not model must not become one that it does"
        );
    }

    /// A string attribute crosses as itself; anything else crosses as
    /// the document it is, so a reader on the other side is not left
    /// guessing whether quotes were data.
    #[test]
    fn attributes_flatten_without_double_quoting_strings() {
        let attributes = Attributes::new(
            [
                ("window".to_owned(), json!("03:00-03:20")),
                ("rows".to_owned(), json!(12)),
            ]
            .into_iter()
            .collect(),
        )
        .expect("valid attributes");

        let flattened = flatten(&attributes);

        assert_eq!(flattened["window"], "03:00-03:20");
        assert_eq!(flattened["rows"], "12");
    }

    #[test]
    fn reading_everything_asks_as_of_a_moment_past_any_session() {
        assert!(end_of_time().instant().year() > 9000);
    }
}
