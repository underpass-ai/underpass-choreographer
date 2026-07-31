//! The memory contract, run against a real memory kernel.
//!
//! Everything else about this adapter can be checked against a stand-in.
//! This cannot: whether a kernel gives back what it was given is a
//! question only the kernel answers, and an adapter that agreed with a
//! stand-in and disagreed with the kernel would pass every other test
//! here.
//!
//! The kernel is a separate binary and is not assumed present. Point
//! `CHOREO_KMP_KERNEL_BIN` at one, or have `rehydration-mcp` on the
//! path, and these run; otherwise they say what they did not do rather
//! than reporting a pass nobody earned.

#![cfg(feature = "kmp")]

use std::time::Duration;

use choreo_adapters::kmp::{
    KernelSessionMemory, KernelTransportError, StdioKernelTransport, StdioKernelTransportConfig,
};
use choreo_core::conformance::MemoryConformance;
use choreo_core::ports::{
    MemoryReaderPort, MemoryRecollection, MemoryWriteOutcome, MemoryWriterPort,
};
use choreo_core::value_objects::{
    Attributes, CeremonyId, MemoryCapability, MemoryDimension, MemoryEntry, MemoryEntryKind,
    MemoryEvidence, MemoryMoment, MemoryProvenance, MemoryScope, RoleId,
};
use time::OffsetDateTime;

/// A kernel of its own, on a data directory of its own.
///
/// One writer per directory is the kernel's rule, so every test that
/// wants a kernel gets a fresh one rather than sharing.
///
/// Whether a missing kernel skips or fails depends on who said it
/// would be there. Asked for by name and absent is a broken setup and
/// fails; absent with nobody having promised it is the ordinary case
/// on a machine that has not built the kernel, and skips.
async fn live_memory() -> Option<(KernelSessionMemory<StdioKernelTransport>, tempfile::TempDir)> {
    let named = std::env::var("CHOREO_KMP_KERNEL_BIN").ok();
    let data_dir = tempfile::tempdir().expect("a temporary data directory");
    let mut config = StdioKernelTransportConfig::new(data_dir.path())
        .expect("a valid data directory")
        .with_call_timeout(Duration::from_mins(1));
    if let Some(binary) = named.clone() {
        config = config.with_binary(binary);
    }

    match StdioKernelTransport::connect(&config).await {
        Ok(transport) => Some((KernelSessionMemory::new(transport), data_dir)),
        Err(KernelTransportError::Unstartable(why)) if named.is_none() => {
            eprintln!(
                "skipped: no memory kernel on the path ({why}); \
                 set CHOREO_KMP_KERNEL_BIN to run this against one"
            );
            None
        }
        Err(error) => panic!("a memory kernel was named and could not be started: {error}"),
    }
}

macro_rules! kernel_or_skip {
    () => {
        match live_memory().await {
            Some(pair) => pair,
            None => return,
        }
    };
}

fn moment(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(seconds)
}

fn entry(summary: &str, kind: MemoryEntryKind, at: i64) -> MemoryEntry {
    MemoryEntry::new(
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
async fn a_real_kernel_satisfies_the_memory_contract() {
    let (memory, _data_dir) = kernel_or_skip!();

    let passed = MemoryConformance::run(&memory, &memory)
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(passed.len(), 9, "{passed:?}");
}

/// The capabilities claimed are the ones the suite then holds the
/// adapter to, so they are worth stating in a test of their own.
#[tokio::test]
async fn a_kernel_backed_memory_declares_what_it_does() {
    let (memory, _data_dir) = kernel_or_skip!();

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
async fn provenance_and_strand_survive_a_real_round_trip() {
    let (memory, _data_dir) = kernel_or_skip!();
    let scope = MemoryScope::new("ceremony:provenance").expect("a valid scope");

    let written = MemoryEntry::new(
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
        .remember(&scope, vec![written], "live:provenance")
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
async fn a_real_kernel_reads_memory_as_it_stood() {
    let (memory, _data_dir) = kernel_or_skip!();
    let scope = MemoryScope::new("ceremony:as-it-stood").expect("a valid scope");

    memory
        .remember(
            &scope,
            vec![
                entry(
                    "the queue was backing up",
                    MemoryEntryKind::Observation,
                    100,
                ),
                entry("the cause was a bad deploy", MemoryEntryKind::Outcome, 900),
            ],
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
async fn a_repeated_write_is_refused_by_a_real_kernel_and_read_as_already_remembered() {
    let (memory, _data_dir) = kernel_or_skip!();
    let scope = MemoryScope::new("ceremony:retried").expect("a valid scope");
    let entries = || vec![entry("decided once", MemoryEntryKind::Decision, 60)];

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
async fn a_session_scoped_to_its_own_ceremony_round_trips() {
    let (memory, _data_dir) = kernel_or_skip!();
    let ceremony = CeremonyId::new("session-42").expect("a valid ceremony id");
    let scope = MemoryScope::of_ceremony(&ceremony).expect("a valid scope");

    let written = MemoryEntry::new(
        MemoryEntryKind::Outcome,
        "the deploy was rolled back and the queue drained",
        None,
        MemoryProvenance::new(ceremony.clone(), None, moment(300)),
        Attributes::empty(),
    )
    .expect("a valid entry");

    memory
        .remember(&scope, vec![written], "live:own-ceremony")
        .await
        .expect("a session should be able to remember under its own name");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let [entry] = recalled.entries() else {
        panic!("expected exactly one entry, got {:?}", recalled.entries());
    };
    assert_eq!(entry.provenance().ceremony_id(), &ceremony);
}

/// Evidence crosses the boundary; its source does not.
///
/// The module says so in prose, and this is the prose made falsifiable.
/// If a later kernel starts returning what it was given, this fails and
/// the limitation gets removed from the documentation on purpose rather
/// than lingering as a stale warning.
#[tokio::test]
async fn evidence_crosses_but_its_source_and_detail_do_not() {
    let (memory, _data_dir) = kernel_or_skip!();
    let scope = MemoryScope::new("ceremony:evidence").expect("a valid scope");

    let detail = Attributes::new(
        [("window".to_owned(), serde_json::json!("03:00-03:20"))]
            .into_iter()
            .collect(),
    )
    .expect("valid attributes");
    let written = MemoryEntry::new(
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
        .remember(&scope, vec![written], "live:evidence")
        .await
        .expect("the write should be accepted");

    let recalled = memory.recall(&scope).await.expect("the read should work");
    let [entry] = recalled.entries() else {
        panic!("expected exactly one entry, got {:?}", recalled.entries());
    };

    let [evidence] = entry.evidence() else {
        panic!(
            "expected exactly one evidence item, got {:?}",
            entry.evidence()
        );
    };
    assert_eq!(evidence.label(), "dead-letter count was zero");
    assert_eq!(
        evidence.source_id(),
        None,
        "the kernel's read surface does not return an evidence source"
    );
    assert!(
        entry.detail().is_empty(),
        "the kernel's read surface does not return an entry's detail"
    );
}

/// A session nobody has written about is empty, not broken.
///
/// The kernel refuses a read on a memory it has never heard of, and
/// this is the one refusal the adapter is allowed to read as
/// emptiness — so it is worth proving against the real wording rather
/// than against a stand-in that says what this adapter expects.
#[tokio::test]
async fn an_unwritten_scope_is_empty_against_a_real_kernel() {
    let (memory, _data_dir) = kernel_or_skip!();
    let scope = MemoryScope::new("ceremony:never-written").expect("a valid scope");

    let recalled = memory.recall(&scope).await.expect("the read should work");

    assert_eq!(recalled, MemoryRecollection::Recalled(Vec::new()));
    assert!(
        MemoryReaderPort::capabilities(&memory).has(MemoryCapability::Recalling),
        "an empty answer only means something from a backend that claims to recall"
    );
}
