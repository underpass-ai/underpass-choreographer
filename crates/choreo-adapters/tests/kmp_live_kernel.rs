//! The memory contract, run against a real memory kernel.
//!
//! Everything else about this adapter can be checked against a stand-in.
//! This cannot: whether a kernel gives back what it was given is a
//! question only the kernel answers, and an adapter that agreed with a
//! stand-in and disagreed with the kernel would pass every other test
//! here.
//!
//! # These cannot pass without an embedded kernel
//!
//! The kernel is another repository's binary and is not assumed
//! present, so every test here is ignored by default. That is the
//! whole point: a run without one reports `0 passed; N ignored`, and
//! the number is the only thing a reader of a CI log actually looks
//! at. They previously ran by default and skipped, which reported
//! `N passed` for a file named after a kernel it had never started.
//!
//! Running them is therefore a deliberate act:
//!
//! ```text
//! cargo test -p choreo-adapters --features kmp --test kmp_live_kernel -- --include-ignored
//! ```
//!
//! And asking for them without a kernel **fails**. There is
//! deliberately no way to be let off: whoever types
//! `--include-ignored` is asking for the kernel to be exercised, and
//! an escape hatch would report `N passed` for a run that started
//! nothing — the very thing this arrangement exists to remove. Anyone
//! who cannot run them already has one: not passing the flag.
//!
//! The property worth stating plainly, because it is what makes the
//! green mean something: **these tests cannot report success without
//! an embedded kernel.**
//!
//! Embedded, and only embedded. The transport starts the kernel's
//! single-binary edition and nothing else, so that is the edition this
//! contract is ever proven against — while the service edition is the
//! one its authors consider the finished article. Everything green
//! here is green about the smaller of the two, which is worth
//! remembering before quoting it as evidence about the other.
//!
//! `CHOREO_KMP_KERNEL_BIN` points at a particular binary; otherwise
//! `rehydration-mcp` is looked for on the path.

#![cfg(feature = "kmp")]

use std::time::Duration;

use choreo_adapters::kmp::{KernelSessionMemory, StdioKernelTransport, StdioKernelTransportConfig};
use choreo_core::conformance::MemoryConformance;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    Attributes, CeremonyId, MemoryCapability, MemoryConfidence, MemoryDimension, MemoryEntry,
    MemoryEntryId, MemoryEntryKind, MemoryEvidence, MemoryMoment, MemoryProvenance, MemoryRelation,
    MemoryRelationKind, MemoryScope, MemoryWrite, RoleId,
};
use time::OffsetDateTime;

/// A kernel of its own, on a data directory of its own.
///
/// One writer per directory is the kernel's rule, so every test that
/// wants a kernel gets a fresh one rather than sharing.
///
/// There is no path through here that hands back a working memory
/// without a kernel behind it, and none that returns quietly when
/// there is not one. Either is the same lie in different clothes.
async fn live_memory() -> (KernelSessionMemory<StdioKernelTransport>, tempfile::TempDir) {
    let data_dir = tempfile::tempdir().expect("a temporary data directory");
    let mut config = StdioKernelTransportConfig::new(data_dir.path())
        .expect("a valid data directory")
        .with_call_timeout(Duration::from_mins(1));
    if let Ok(binary) = std::env::var("CHOREO_KMP_KERNEL_BIN") {
        config = config.with_binary(binary);
    }

    match StdioKernelTransport::connect(&config).await {
        Ok(transport) => (KernelSessionMemory::new(transport), data_dir),
        Err(error) => panic!(
            "these tests were asked for and no memory kernel could be started: {error}\n\
             install one — `cargo install --path crates/rehydration-mcp` in the kernel \
             repository — or point CHOREO_KMP_KERNEL_BIN at one"
        ),
    }
}

fn moment(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(seconds)
}

fn id(raw: &str) -> MemoryEntryId {
    MemoryEntryId::new(raw).expect("a valid entry id")
}

/// Entries with nothing connecting them, for the checks that are not
/// about reasons.
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
            CeremonyId::new("live-kernel").expect("a valid ceremony id"),
            None,
            moment(at),
        ),
        Attributes::empty(),
    )
    .expect("a valid entry")
}

#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_real_kernel_satisfies_the_memory_contract() {
    let (memory, _data_dir) = live_memory().await;

    let passed = MemoryConformance::run(&memory, &memory)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 10, "{passed:?}");
}

/// The capabilities claimed are the ones the suite then holds the
/// adapter to, so they are worth stating in a test of their own.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_kernel_backed_memory_declares_what_it_does() {
    let (memory, _data_dir) = live_memory().await;

    let capabilities = MemoryWriterPort::capabilities(&memory);

    assert!(capabilities.remembers());
    assert!(capabilities.recalls());
    assert!(capabilities.travels_in_time());
    assert!(capabilities.keeps_evidence());
    assert!(
        !capabilities.answers_questions(),
        "asking is declined for now; the kernel answers with a proof this port cannot carry"
    );
}

/// What a session writes comes back with the axis, the author and the
/// moment it was written under — the whole point of putting provenance
/// on dimensions rather than in a metadata bag.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn provenance_and_strand_survive_a_real_round_trip() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:provenance").expect("a valid scope");

    let written = MemoryEntry::new(
        id("no-double-restart"),
        MemoryEntryKind::Constraint,
        "the ingester may not be restarted twice in an hour",
        Some(MemoryDimension::new("timeline").expect("a valid dimension")),
        MemoryProvenance::new(
            CeremonyId::new("session-7").expect("a valid ceremony id"),
            Some(RoleId::new("responder").expect("a valid role id")),
            moment(120),
        ),
        Attributes::empty(),
    )
    .expect("a valid entry");

    memory
        .remember(&scope, write(vec![written]), "live:provenance")
        .await
        .expect("the write should be accepted");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let [entry] = recalled.entries() else {
        panic!("expected exactly one entry, got {:?}", recalled.entries());
    };

    assert_eq!(entry.kind(), MemoryEntryKind::Constraint);
    assert_eq!(
        entry.summary(),
        "the ingester may not be restarted twice in an hour"
    );
    assert_eq!(
        entry.dimension().map(MemoryDimension::as_str),
        Some("timeline")
    );
    assert_eq!(entry.provenance().ceremony_id().as_str(), "session-7");
    assert_eq!(
        entry.provenance().role_id().map(RoleId::as_str),
        Some("responder")
    );
    assert_eq!(entry.provenance().observed_at(), moment(120));
}

/// Reading memory as of a moment must exclude what was learned after
/// it, and this is the adapter's own claim rather than the suite's
/// smallest case: two writes, minutes apart, read from between them.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_real_kernel_reads_memory_as_it_stood() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:as-it-stood").expect("a valid scope");

    memory
        .remember(
            &scope,
            write(vec![
                entry(
                    "the queue was backing up",
                    MemoryEntryKind::Observation,
                    100,
                ),
                entry("the cause was a bad deploy", MemoryEntryKind::Outcome, 900),
            ]),
            "live:as-it-stood",
        )
        .await
        .expect("the write should be accepted");

    let earlier = memory
        .as_known_at(&scope, MemoryMoment::at(moment(500)))
        .await
        .expect("the read should work");

    let summaries: Vec<_> = earlier.entries().iter().map(MemoryEntry::summary).collect();
    assert_eq!(summaries, vec!["the queue was backing up"]);

    let everything = memory.recall(&scope).await.expect("the read should work");
    assert_eq!(everything.entries().len(), 2);
}

/// A retry must not double the memory, and the kernel must be the one
/// saying so — this is the property whose real behaviour differs from
/// what a reader of the port would guess.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_repeated_write_is_refused_by_a_real_kernel_and_read_as_already_remembered() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:retried").expect("a valid scope");
    let entries = || write(vec![entry("decided once", MemoryEntryKind::Decision, 60)]);

    let first = memory
        .remember(&scope, entries(), "live:retried")
        .await
        .expect("the first write should be accepted");
    let second = memory
        .remember(&scope, entries(), "live:retried")
        .await
        .expect("the retry should be an outcome, not an error");

    assert_eq!(first, MemoryWriteOutcome::Remembered);
    assert_eq!(second, MemoryWriteOutcome::AlreadyRemembered);
    assert_eq!(
        memory
            .recall(&scope)
            .await
            .expect("the read should work")
            .entries()
            .len(),
        1
    );
}

/// The ordinary case: a session's memory scoped to the very ceremony
/// that is writing it.
///
/// [`MemoryScope::of_ceremony`] names a scope after a ceremony, and an
/// entry written in that session names the same ceremony in its
/// provenance. Both become dimensions, so both need names that cannot
/// be each other's — which is only obvious once the two ids are built
/// the same way and turn out identical.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_session_scoped_to_its_own_ceremony_round_trips() {
    let (memory, _data_dir) = live_memory().await;
    let ceremony = CeremonyId::new("session-42").expect("a valid ceremony id");
    let scope = MemoryScope::of_ceremony(&ceremony).expect("a valid scope");

    let written = MemoryEntry::new(
        id("outcome"),
        MemoryEntryKind::Outcome,
        "the deploy was rolled back and the queue drained",
        None,
        MemoryProvenance::new(ceremony.clone(), None, moment(300)),
        Attributes::empty(),
    )
    .expect("a valid entry");

    memory
        .remember(&scope, write(vec![written]), "live:own-ceremony")
        .await
        .expect("a session should be able to remember under its own name");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let [entry] = recalled.entries() else {
        panic!("expected exactly one entry, got {:?}", recalled.entries());
    };
    assert_eq!(entry.provenance().ceremony_id(), &ceremony);
}

/// Evidence crosses whole: its label, where it came from, and what
/// was hung on it.
///
/// This test used to say the opposite. It pinned a loss — the kernel
/// took a source and an entry's detail and gave neither back — and
/// said in as many words that the day they came back it would fail
/// and the limitation would be deleted on purpose rather than linger
/// as a stale warning.
///
/// That is what happened, and it is worth recording that the tripwire
/// did not go off on its own: the adapter was still dropping both on
/// the way in, so the assertion never saw them. A test that pins a
/// limitation has to read past the code that implements it, or it
/// pins the workaround instead.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn evidence_crosses_whole_with_its_source_and_detail() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:evidence").expect("a valid scope");

    let detail = Attributes::new(
        [("window".to_owned(), serde_json::json!("03:00-03:20"))]
            .into_iter()
            .collect(),
    )
    .expect("valid attributes");
    let written = MemoryEntry::new(
        id("queue-empty"),
        MemoryEntryKind::Observation,
        "the dead-letter queue was empty",
        None,
        MemoryProvenance::new(
            CeremonyId::new("session-9").expect("a valid ceremony id"),
            None,
            moment(200),
        ),
        detail,
    )
    .expect("a valid entry")
    .with_evidence(vec![MemoryEvidence::new(
        "dead-letter count was zero",
        Some("dead-letter-queue".to_owned()),
        Attributes::empty(),
    )
    .expect("valid evidence")]);

    memory
        .remember(&scope, write(vec![written]), "live:evidence")
        .await
        .expect("the write should be accepted");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let [entry] = recalled.entries() else {
        panic!("expected exactly one entry, got {:?}", recalled.entries());
    };

    assert_eq!(
        entry
            .detail()
            .get("window")
            .and_then(serde_json::Value::as_str),
        Some("03:00-03:20"),
        "what was hung on the claim came back with it"
    );

    let [evidence] = entry.evidence() else {
        panic!(
            "expected exactly one evidence item, got {:?}",
            entry.evidence()
        );
    };
    assert_eq!(evidence.label(), "dead-letter count was zero");
    assert_eq!(
        evidence.source_id(),
        Some("dead-letter-queue"),
        "a citation without its reference is what this was waiting for"
    );
}

/// A session nobody has written about is empty, not broken.
///
/// The kernel refuses a read on a memory it has never heard of, and
/// this is the one refusal the adapter is allowed to read as
/// emptiness — so it is worth proving against the real wording rather
/// than against a stand-in that says what this adapter expects.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn an_unwritten_scope_is_empty_against_a_real_kernel() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:never-written").expect("a valid scope");

    let recalled = memory.recall(&scope).await.expect("the read should work");

    assert_eq!(recalled, MemoryRecollection::nothing());
    assert!(
        MemoryReaderPort::capabilities(&memory).has(MemoryCapability::Recalling),
        "an empty answer only means something from a backend that claims to recall"
    );
}

/// A reason survives a real kernel, with its words and its degree.
///
/// The point of the whole contract. An entry says what was decided; only
/// the edge says what made it necessary, and a later session works that
/// out by following edges rather than by reading a list.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_reason_survives_a_real_round_trip() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:reasons").expect("a valid scope");
    let observation = entry(
        "the queue was backing up",
        MemoryEntryKind::Observation,
        400,
    );
    let decision = entry(
        "roll back rather than restart",
        MemoryEntryKind::Decision,
        500,
    );
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
            &scope,
            MemoryWrite::new(vec![observation, decision], vec![because])
                .expect("a write with entries and a reason"),
            "live:reasons",
        )
        .await
        .expect("the write should be accepted");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let [reason] = recalled.relations() else {
        panic!(
            "expected exactly one reason, got {:?}",
            recalled.relations()
        );
    };

    assert_eq!(reason.kind(), MemoryRelationKind::ChosenBecause);
    assert_eq!(
        reason.why(),
        "the queue growth is what made a rollback necessary"
    );
    assert_eq!(reason.confidence(), MemoryConfidence::High);
    assert_eq!(reason.from().as_str(), "roll back rather than restart");
    assert_eq!(reason.to().as_str(), "the queue was backing up");
}

/// A reason pointing at something not recalled is not handed back.
///
/// Reading memory as of a moment can leave one end of an explanation
/// in the future. An edge into nothing claims a reason exists and gives
/// no way to reach it, which is worse than admitting there is none.
#[tokio::test]
#[ignore = "needs a memory kernel: run with --include-ignored"]
async fn a_reason_with_one_end_out_of_reach_is_not_returned() {
    let (memory, _data_dir) = live_memory().await;
    let scope = MemoryScope::new("ceremony:half-reason").expect("a valid scope");
    let early = entry("known at the time", MemoryEntryKind::Observation, 100);
    let late = entry("decided later", MemoryEntryKind::Decision, 900);
    let because = MemoryRelation::new(
        late.id().clone(),
        early.id().clone(),
        MemoryRelationKind::ChosenBecause,
        "what was seen early is what settled it",
        MemoryConfidence::Medium,
    )
    .expect("a valid reason");

    memory
        .remember(
            &scope,
            MemoryWrite::new(vec![early, late], vec![because]).expect("a valid write"),
            "live:half-reason",
        )
        .await
        .expect("the write should be accepted");

    let earlier = memory
        .as_known_at(&scope, MemoryMoment::at(moment(500)))
        .await
        .expect("the read should work");

    assert_eq!(earlier.entries().len(), 1);
    assert!(
        earlier.relations().is_empty(),
        "an explanation whose far end is not visible was handed back anyway"
    );
}
