//! The kernel-backed adapter's own logic, without a kernel.
//!
//! What is checked here is what belongs to the adapter rather than to
//! the kernel: that a write says what it means to say, that paging
//! collects everything once, that a refusal is read for what it is,
//! and that memory this engine cannot represent is left out instead of
//! invented. `kmp_live_kernel.rs` is what proves the other half — that
//! a real kernel gives back what it was given — and neither test is a
//! substitute for the other.
//!
//! The stand-in below answers in shapes captured from a real kernel,
//! not in shapes invented to suit the adapter. It is still a model,
//! which is why the counterexamples matter more than the happy path:
//! each one breaks a promise the adapter is supposed to keep whatever
//! the kernel does.

#![cfg(feature = "kmp")]

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use choreo_adapters::kmp::{
    KernelAnswer, KernelSessionMemory, KernelTransport, KernelTransportError,
};
use choreo_core::conformance::MemoryConformance;
use choreo_core::ports::{MemoryReaderPort, MemoryRecollection, MemoryWriterPort};
use choreo_core::value_objects::{
    Attributes, CeremonyId, MemoryConfidence, MemoryDimension, MemoryEntry, MemoryEntryId,
    MemoryEntryKind, MemoryEvidence, MemoryMoment, MemoryProvenance, MemoryRelation,
    MemoryRelationKind, MemoryScope, MemoryWrite, RoleId,
};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

fn moment(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(seconds)
}

fn scope(name: &str) -> MemoryScope {
    MemoryScope::new(format!("ceremony:{name}")).expect("a valid scope")
}

fn id(raw: &str) -> MemoryEntryId {
    MemoryEntryId::new(raw).expect("a valid entry id")
}

fn write(entries: Vec<MemoryEntry>) -> MemoryWrite {
    MemoryWrite::unexplained(entries).expect("a write with entries is valid")
}

fn entry(summary: &str, kind: MemoryEntryKind, at: i64) -> MemoryEntry {
    MemoryEntry::new(
        id(summary),
        kind,
        summary,
        None,
        MemoryProvenance::new(
            CeremonyId::new("stand-in").expect("a valid ceremony id"),
            None,
            moment(at),
        ),
        Attributes::empty(),
    )
    .expect("a valid entry")
}

// ---------------------------------------------------------------------
// A stand-in for a memory kernel
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct StoredEntry {
    reference: String,
    kind: String,
    text: String,
    coordinates: Value,
    occurred_at: OffsetDateTime,
}

/// A kernel's worth of behaviour, in a mutex.
///
/// It keeps the parts of the protocol this adapter uses and none of
/// the rest: memory under a name, a write that will not be made twice
/// under one key, and a temporal read that pages.
#[derive(Debug, Default)]
struct KernelStandIn {
    written: Mutex<BTreeMap<String, Vec<StoredEntry>>>,
    keys: Mutex<BTreeMap<String, String>>,
    supports: Mutex<Vec<(String, String)>>,
    reasons: Mutex<Vec<Value>>,
    page_size: usize,
    /// Requests seen, so a test can say what the adapter asked for.
    calls: Mutex<Vec<(String, Value)>>,
}

impl KernelStandIn {
    fn new(page_size: usize) -> Self {
        Self {
            page_size,
            ..Self::default()
        }
    }

    fn ingest(&self, arguments: &Value) -> KernelAnswer {
        let about = arguments["about"].as_str().expect("an about").to_owned();
        let key = arguments["idempotency_key"]
            .as_str()
            .expect("a key")
            .to_owned();

        // The kernel translates a write against what it already holds,
        // so a replay never hashes to the original and is refused with
        // its state left as it was.
        if self.keys.lock().unwrap().contains_key(&key) {
            return KernelAnswer::Refused(format!(
                "embedded kernel ingest failed for `{about}`: idempotency key '{key}' \
                 was already accepted with different content"
            ));
        }
        self.keys.lock().unwrap().insert(key, about.clone());

        let mut written = self.written.lock().unwrap();
        let held = written.entry(about.clone()).or_default();
        for value in arguments["memory"]["entries"].as_array().expect("entries") {
            let coordinates = value["coordinates"].as_array().expect("coordinates");
            let occurred_at = coordinates
                .iter()
                .find_map(|c| c["occurred_at"].as_str())
                .map(|raw| OffsetDateTime::parse(raw, &Rfc3339).expect("an RFC3339 time"))
                .expect("a coordinate carrying a time");

            // A scope id comes back namespaced by the memory it is in.
            let namespaced: Vec<Value> = coordinates
                .iter()
                .map(|c| {
                    json!({
                        "dimension": c["dimension"],
                        "scope_id": format!(
                            "about:{about}:dimension:{}",
                            c["scope_id"].as_str().expect("a scope id")
                        ),
                        "sequence": c["sequence"],
                        "occurred_at": c["occurred_at"],
                    })
                })
                .collect();

            held.push(StoredEntry {
                reference: value["id"].as_str().expect("an id").to_owned(),
                kind: value["kind"].as_str().expect("a kind").to_owned(),
                text: value["text"].as_str().expect("text").to_owned(),
                coordinates: Value::Array(namespaced),
                occurred_at,
            });
        }

        for relation in arguments["memory"]["relations"]
            .as_array()
            .into_iter()
            .flatten()
        {
            self.reasons.lock().unwrap().push(relation.clone());
        }

        let mut supports = self.supports.lock().unwrap();
        for item in arguments["memory"]["evidence"]
            .as_array()
            .into_iter()
            .flatten()
        {
            for supported in item["supports"].as_array().into_iter().flatten() {
                supports.push((
                    supported.as_str().expect("a ref").to_owned(),
                    item["text"].as_str().expect("text").to_owned(),
                ));
            }
        }

        KernelAnswer::Returned(json!({
            "memory": { "about": about, "read_after_write_ready": true },
            "warnings": [],
        }))
    }

    fn goto(&self, arguments: &Value) -> KernelAnswer {
        let about = arguments["about"].as_str().expect("an about");
        let written = self.written.lock().unwrap();
        let Some(held) = written.get(about) else {
            return KernelAnswer::Refused(format!(
                "embedded kernel goto failed for `{about}`: node '{about}' not found"
            ));
        };

        let visible: Vec<&StoredEntry> = match arguments["at"]["time"].as_str() {
            Some(raw) => {
                let cursor = OffsetDateTime::parse(raw, &Rfc3339).expect("an RFC3339 cursor");
                held.iter().filter(|e| e.occurred_at <= cursor).collect()
            }
            None => after_cursor(held, arguments["at"]["ref"].as_str()),
        };

        self.page(visible)
    }

    /// The shortest chain of reasons between two refs, breadth-first.
    ///
    /// The stand-in has to do real path-finding here: a trace that
    /// returned the edges it happened to hold would let an adapter that
    /// never walks anything look like one that does.
    fn trace(&self, arguments: &Value) -> KernelAnswer {
        let from = arguments["from"].as_str().unwrap_or_default().to_owned();
        let to = arguments["to"].as_str().unwrap_or_default();
        let reasons = self.reasons.lock().unwrap().clone();

        let mut frontier = std::collections::VecDeque::from([from.clone()]);
        let mut arrived_by: BTreeMap<String, Value> = BTreeMap::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::from([from]);
        while let Some(here) = frontier.pop_front() {
            if here == to {
                break;
            }
            for reason in reasons
                .iter()
                .filter(|reason| reason["from"].as_str() == Some(here.as_str()))
            {
                let next = reason["to"].as_str().unwrap_or_default().to_owned();
                if seen.insert(next.clone()) {
                    arrived_by.insert(next.clone(), reason.clone());
                    frontier.push_back(next);
                }
            }
        }

        let mut chain = Vec::new();
        let mut here = to.to_owned();
        while let Some(reason) = arrived_by.get(&here) {
            chain.push(reason.clone());
            here = reason["from"].as_str().unwrap_or_default().to_owned();
        }
        chain.reverse();

        KernelAnswer::Returned(json!({ "trace": chain, "warnings": [] }))
    }

    fn page(&self, visible: Vec<&StoredEntry>) -> KernelAnswer {
        let total = visible.len();
        let returned: Vec<&StoredEntry> = visible.into_iter().take(self.page_size.max(1)).collect();
        let has_more = total > returned.len();
        let next_cursor = has_more
            .then(|| returned.last().map(|entry| entry.reference.clone()))
            .flatten();

        let visible: std::collections::BTreeSet<&str> =
            returned.iter().map(|e| e.reference.as_str()).collect();
        // The reasons whose two ends are both on this page. A stand-in
        // that returned dangling edges would let the adapter look
        // better than it is.
        let mut path: Vec<Value> = self
            .reasons
            .lock()
            .unwrap()
            .iter()
            .filter(|reason| {
                visible.contains(reason["from"].as_str().unwrap_or_default())
                    && visible.contains(reason["to"].as_str().unwrap_or_default())
            })
            .cloned()
            .collect();

        let supports = self.supports.lock().unwrap();
        let evidence_path: Vec<Value> = returned
            .iter()
            .flat_map(|entry| {
                supports
                    .iter()
                    .filter(|(supported, _)| *supported == entry.reference)
                    .map(|(supported, text)| {
                        json!({
                            "class": "evidential",
                            "rel": "supports",
                            "from": format!("evidence:{supported}"),
                            "to": supported,
                            "evidence": text,
                            "why": "Evidence supports this memory entry.",
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        path.extend(evidence_path);

        KernelAnswer::Returned(json!({
            "entries": returned.iter().map(|entry| json!({
                "ref": entry.reference,
                "kind": entry.kind,
                "text": entry.text,
                "coordinates": entry.coordinates,
            })).collect::<Vec<_>>(),
            "page": {
                "has_more": has_more,
                "next_cursor": next_cursor,
                "returned": returned.len(),
                "total": total,
            },
            "proof": { "evidence": [], "path": path },
            "warnings": [],
        }))
    }
}

#[async_trait]
impl KernelTransport for KernelStandIn {
    async fn call(
        &self,
        tool: &str,
        arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError> {
        self.calls
            .lock()
            .unwrap()
            .push((tool.to_owned(), arguments.clone()));
        match tool {
            "kernel_ingest" => Ok(self.ingest(&arguments)),
            "kernel_goto" => Ok(self.goto(&arguments)),
            "kernel_trace" => Ok(self.trace(&arguments)),
            other => panic!("the adapter called a tool this stand-in does not know: {other}"),
        }
    }
}

/// Continuing from a reference picks up after the entry named — and
/// says nothing about when, which is the whole point of the
/// counterexample below.
fn after_cursor<'a>(held: &'a [StoredEntry], cursor: Option<&str>) -> Vec<&'a StoredEntry> {
    let Some(cursor) = cursor else {
        return held.iter().collect();
    };
    held.iter()
        .skip_while(|entry| entry.reference != cursor)
        .skip(1)
        .collect()
}

/// A stand-in the test keeps a handle on.
///
/// The adapter owns its transport, which is right — a memory that
/// could be reconfigured from outside would be a memory nobody could
/// reason about. So a test that wants to see what was asked holds the
/// stand-in itself and lends it out.
#[derive(Debug, Clone)]
struct Shared(std::sync::Arc<KernelStandIn>);

#[async_trait]
impl KernelTransport for Shared {
    async fn call(
        &self,
        tool: &str,
        arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError> {
        self.0.call(tool, arguments).await
    }
}

// ---------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------

#[tokio::test]
async fn the_adapter_satisfies_the_contract_against_a_stand_in() {
    let memory = KernelSessionMemory::new(KernelStandIn::new(2));

    let passed = MemoryConformance::run(&memory, &memory)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 10, "{passed:?}");
}

/// Provenance rides on dimensions, and this is the test that says so
/// in the request rather than in a comment: an entry's session, the
/// ceremony that produced it, the role that saw it and the strand it
/// runs along are all coordinates the kernel can navigate later.
#[tokio::test]
async fn a_write_carries_provenance_as_dimensions() {
    let kernel = std::sync::Arc::new(KernelStandIn::new(50));
    let memory = KernelSessionMemory::new(Shared(kernel.clone()));
    let written = MemoryEntry::new(
        id("roll-back"),
        MemoryEntryKind::Decision,
        "roll back to the previous revision",
        Some(MemoryDimension::new("timeline").expect("a valid dimension")),
        MemoryProvenance::new(
            CeremonyId::new("session-3").expect("a valid ceremony id"),
            Some(RoleId::new("responder").expect("a valid role id")),
            moment(90),
        ),
        Attributes::empty(),
    )
    .expect("a valid entry");

    memory
        .remember(
            &scope("dimensions"),
            write(vec![written]),
            "write:dimensions",
        )
        .await
        .expect("the write should be accepted");

    let calls = kernel.calls.lock().unwrap().clone();
    let (tool, arguments) = calls.first().expect("one call");
    assert_eq!(tool, "kernel_ingest");

    let declared: Vec<&str> = arguments["memory"]["dimensions"]
        .as_array()
        .expect("dimensions")
        .iter()
        .map(|d| d["kind"].as_str().expect("a kind"))
        .collect();
    assert_eq!(declared, vec!["ceremony", "role", "session", "strand"]);

    let coordinates = arguments["memory"]["entries"][0]["coordinates"]
        .as_array()
        .expect("coordinates");
    let by_dimension: BTreeMap<&str, &str> = coordinates
        .iter()
        .map(|c| {
            (
                c["dimension"].as_str().expect("a dimension"),
                c["scope_id"].as_str().expect("a scope id"),
            )
        })
        .collect();
    assert_eq!(by_dimension["ceremony"], "ceremony:session-3");
    assert_eq!(by_dimension["role"], "role:responder");
    assert_eq!(by_dimension["strand"], "strand:timeline");
    assert_eq!(
        by_dimension["session"], "session:ceremony:dimensions",
        "the session's own axis is qualified like every other, so a scope \
         named after a ceremony cannot collide with that ceremony's axis"
    );
}

/// A session's memory outgrows one page long before it outgrows the
/// engine, so reading it has to keep going — once per entry, and no
/// more than once.
#[tokio::test]
async fn reading_walks_every_page_and_repeats_nothing() {
    let memory = KernelSessionMemory::new(KernelStandIn::new(2));
    let scope = scope("paged");
    let entries: Vec<MemoryEntry> = (1..=7)
        .map(|n| entry(&format!("thing {n}"), MemoryEntryKind::Observation, n * 10))
        .collect();

    memory
        .remember(&scope, write(entries), "write:paged")
        .await
        .expect("the write should be accepted");

    let recalled = memory.recall(&scope).await.expect("the read should work");

    let summaries: Vec<&str> = recalled
        .entries()
        .iter()
        .map(MemoryEntry::summary)
        .collect();
    assert_eq!(
        summaries,
        vec!["thing 1", "thing 2", "thing 3", "thing 4", "thing 5", "thing 6", "thing 7"]
    );
}

// ---------------------------------------------------------------------
// Counterexamples: kernels that break a promise
// ---------------------------------------------------------------------

/// Paging by reference does not carry the moment, so the adapter must.
///
/// The first page of a temporal read is asked for by time and every
/// page after it by reference, and a reference says nothing about when
/// — so a kernel continuing from one has no reason to keep excluding
/// what was learned later. The stand-in behaves exactly that way, not
/// as a fault injected for the test but because that is what
/// continuing from a reference means.
///
/// Two entries must already be known at the moment asked about, or the
/// first page is the only page and no second one is ever fetched. A
/// counterexample that cannot reach the code it accuses proves
/// nothing, and this one had to be caught being too easy first.
#[tokio::test]
async fn paging_cannot_smuggle_in_what_was_learned_later() {
    let memory = KernelSessionMemory::new(KernelStandIn::new(1));
    let scope = scope("careless");

    memory
        .remember(
            &scope,
            write(vec![
                entry("known at the time", MemoryEntryKind::Observation, 100),
                entry("also known by then", MemoryEntryKind::Observation, 200),
                entry("known only later", MemoryEntryKind::Outcome, 900),
            ]),
            "write:careless",
        )
        .await
        .expect("the write should be accepted");

    let recalled = memory
        .as_known_at(&scope, MemoryMoment::at(moment(500)))
        .await
        .expect("the read should work");

    let summaries: Vec<&str> = recalled
        .entries()
        .iter()
        .map(MemoryEntry::summary)
        .collect();
    assert_eq!(
        summaries,
        vec!["known at the time", "also known by then"],
        "reading memory as of a moment returned something learned after it"
    );
}

/// A kernel that refuses for a reason the adapter does not know.
///
/// The tempting reading of any refusal is "there is nothing there",
/// and it is the one reading that must never be taken: a broken kernel
/// would then be indistinguishable from a session nobody wrote about.
#[derive(Debug)]
struct RefusesForAnotherReason;

#[async_trait]
impl KernelTransport for RefusesForAnotherReason {
    async fn call(
        &self,
        _tool: &str,
        _arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError> {
        Ok(KernelAnswer::Refused(
            "embedded kernel goto failed for `ceremony:x`: store is locked by another writer"
                .to_owned(),
        ))
    }
}

#[tokio::test]
async fn a_refusal_that_is_not_emptiness_is_not_read_as_emptiness() {
    let memory = KernelSessionMemory::new(RefusesForAnotherReason);

    let outcome = memory.recall(&scope("locked")).await;

    assert!(
        outcome.is_err(),
        "a kernel that refused for another reason was read as an empty session: {outcome:?}"
    );
}

/// A kernel unreachable altogether is an error, not silence.
#[derive(Debug)]
struct Unreachable;

#[async_trait]
impl KernelTransport for Unreachable {
    async fn call(
        &self,
        _tool: &str,
        _arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError> {
        Err(KernelTransportError::Gone)
    }
}

#[tokio::test]
async fn a_kernel_that_is_gone_is_not_an_empty_session() {
    let memory = KernelSessionMemory::new(Unreachable);

    assert!(memory.recall(&scope("gone")).await.is_err());
    assert!(memory
        .remember(
            &scope("gone"),
            write(vec![entry("x", MemoryEntryKind::Decision, 1)]),
            "k"
        )
        .await
        .is_err());
}

/// Memory another writer left in the same scope.
///
/// A kind this engine does not model cannot be turned into one of the
/// four without inventing what somebody meant, so it is left out — and
/// everything alongside it still comes back, because one foreign entry
/// is not a reason to lose a session's own memory.
#[derive(Debug)]
struct HoldsForeignMemory;

#[async_trait]
impl KernelTransport for HoldsForeignMemory {
    async fn call(
        &self,
        _tool: &str,
        _arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError> {
        Ok(KernelAnswer::Returned(json!({
            "entries": [
                {
                    "ref": "entry:ceremony:shared:from-elsewhere",
                    "kind": "claim",
                    "text": "written by something that is not this engine",
                    "coordinates": [{
                        "dimension": "ceremony",
                        "scope_id": "about:ceremony:shared:dimension:ceremony:other",
                        "occurred_at": "2026-01-01T00:00:10Z",
                    }],
                },
                {
                    "ref": "entry:ceremony:shared:ours",
                    "kind": "decision",
                    "text": "written by this engine",
                    "coordinates": [{
                        "dimension": "ceremony",
                        "scope_id": "about:ceremony:shared:dimension:ceremony:ours",
                        "occurred_at": "2026-01-01T00:00:20Z",
                    }],
                },
            ],
            "page": { "has_more": false, "next_cursor": null },
            "proof": { "evidence": [], "path": [] },
        })))
    }
}

#[tokio::test]
async fn memory_this_engine_cannot_represent_is_left_out_not_invented() {
    let memory = KernelSessionMemory::new(HoldsForeignMemory);

    let recalled = memory
        .recall(&scope("shared"))
        .await
        .expect("the read should work");

    let MemoryRecollection::Recalled { entries, .. } = recalled else {
        panic!("a backend that declares recalling answered unsupported");
    };
    let summaries: Vec<&str> = entries.iter().map(MemoryEntry::summary).collect();
    assert_eq!(summaries, vec!["written by this engine"]);
    assert_eq!(
        entries[0].provenance().ceremony_id().as_str(),
        "ours",
        "provenance should be read back off the dimension it was written on"
    );
}

/// Evidence arrives attached to the entry it backs, not in a pile.
#[tokio::test]
async fn evidence_comes_back_attached_to_its_own_entry() {
    let memory = KernelSessionMemory::new(KernelStandIn::new(50));
    let scope = scope("evidenced");
    let backed =
        entry("the queue was empty", MemoryEntryKind::Observation, 20).with_evidence(vec![
            MemoryEvidence::new("dead-letter count was zero", None, Attributes::empty())
                .expect("valid evidence"),
        ]);

    memory
        .remember(
            &scope,
            write(vec![
                backed,
                entry("unbacked", MemoryEntryKind::Decision, 30),
            ]),
            "write:evidenced",
        )
        .await
        .expect("the write should be accepted");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let entries = recalled.entries();

    assert_eq!(entries[0].evidence().len(), 1);
    assert_eq!(
        entries[0].evidence()[0].label(),
        "dead-letter count was zero"
    );
    assert!(entries[1].evidence().is_empty());
}

/// A reason leaves in the kernel's terms, not in ours.
///
/// How a kernel classes an explanation is its taxonomy, and the mapping
/// belongs in the adapter. This is the test that says which words go
/// out, so a change to them is a decision somebody made rather than a
/// rename that slipped through.
#[tokio::test]
async fn a_reason_is_written_in_the_kernels_terms() {
    let kernel = std::sync::Arc::new(KernelStandIn::new(50));
    let memory = KernelSessionMemory::new(Shared(kernel.clone()));
    let observation = entry("the queue was backing up", MemoryEntryKind::Observation, 10);
    let decision = entry("roll back", MemoryEntryKind::Decision, 20);
    let because = MemoryRelation::new(
        decision.id().clone(),
        observation.id().clone(),
        MemoryRelationKind::ChosenBecause,
        "the queue growth is what made a rollback necessary",
        MemoryConfidence::High,
    )
    .expect("a valid reason");

    memory
        .remember(
            &scope("terms"),
            MemoryWrite::new(vec![observation, decision], vec![because]).expect("a valid write"),
            "write:terms",
        )
        .await
        .expect("the write should be accepted");

    let calls = kernel.calls.lock().unwrap().clone();
    let (_, arguments) = calls.first().expect("one call");
    let relation = &arguments["memory"]["relations"][0];

    assert_eq!(relation["rel"], "chosen_because");
    assert_eq!(
        relation["class"], "motivational",
        "an explanation's class is what decides whether it survives a budget"
    );
    assert_eq!(
        relation["why"], "the queue growth is what made a rollback necessary",
        "the reason travels on the edge, which is the only place it means anything"
    );
    assert_eq!(relation["confidence"], "high");
    assert_eq!(relation["from"], "entry:ceremony:terms:roll back");
}
