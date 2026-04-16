//! Integration test: NATS messaging roundtrip over a real broker.
//!
//! Spins up a `nats:2` testcontainer, publishes every outbound event
//! type via [`NatsMessaging`], subscribes with a raw client, and
//! verifies the payloads land on the canonical subjects with an
//! AsyncAPI-shaped JSON (envelope fields flat at the root).
//!
//! Runs only when the `container-tests` feature is enabled (CI).

#![cfg(feature = "container-tests")]

use std::time::Duration;

use choreo_adapters::nats::{NatsMessaging, NatsSubjects};
use choreo_core::events::{
    DeliberationCompletedEvent, EventEnvelope, PhaseChangedEvent, TaskCompletedEvent,
    TaskDispatchedEvent, TaskFailedEvent,
};
use choreo_core::ports::MessagingPort;
use choreo_core::value_objects::{
    AgentId, DurationMs, EventId, ProposalId, Score, Specialty, TaskId,
};
use futures::StreamExt;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use time::OffsetDateTime;

const NATS_IMAGE: &str = "nats";
const NATS_TAG: &str = "2";

async fn start_nats() -> (
    async_nats::Client,
    testcontainers::ContainerAsync<GenericImage>,
    String,
) {
    let container = GenericImage::new(NATS_IMAGE, NATS_TAG)
        .with_exposed_port(4222_u16.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Server is ready"))
        .with_cmd(["-js"])
        .start()
        .await
        .expect("nats container should start");
    let port = container
        .get_host_port_ipv4(4222_u16.tcp())
        .await
        .expect("host port");
    let url = format!("nats://127.0.0.1:{port}");

    // Retry the connection briefly — testcontainers' wait-for only
    // guarantees stderr progress, the accept loop may still be
    // stabilising.
    let mut last_err = None;
    for _ in 0..20 {
        match async_nats::connect(&url).await {
            Ok(client) => return (client, container, url),
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    panic!("could not connect to nats after warmup: {last_err:?}");
}

fn envelope() -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(uuid::Uuid::new_v4().to_string()).unwrap(),
        OffsetDateTime::now_utc(),
        "integration-test",
        None,
    )
    .unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // a single integration scenario covers all 5 publish methods
async fn all_outbound_events_land_on_their_canonical_subjects() {
    let (client, _container, _url) = start_nats().await;
    let subjects = NatsSubjects::new("choreo", "choreo.trigger.>").unwrap();
    let messaging = NatsMessaging::new(client.clone(), subjects.clone());

    let mut sub_dispatched = client
        .subscribe(subjects.task_dispatched.clone())
        .await
        .unwrap();
    let mut sub_completed = client
        .subscribe(subjects.task_completed.clone())
        .await
        .unwrap();
    let mut sub_failed = client
        .subscribe(subjects.task_failed.clone())
        .await
        .unwrap();
    let mut sub_deliberation = client
        .subscribe(subjects.deliberation_completed.clone())
        .await
        .unwrap();
    let mut sub_phase = client
        .subscribe(subjects.phase_changed.clone())
        .await
        .unwrap();

    // Flush subscribes — NATS is fire-and-forget; wait until the
    // server acknowledges the SUB.
    client.flush().await.unwrap();

    // Publish every event.
    messaging
        .publish_task_dispatched(&TaskDispatchedEvent::new(
            envelope(),
            TaskId::new("t1").unwrap(),
            Specialty::new("triage").unwrap(),
            None,
        ))
        .await
        .unwrap();
    messaging
        .publish_task_completed(&TaskCompletedEvent::new(
            envelope(),
            TaskId::new("t1").unwrap(),
            Specialty::new("triage").unwrap(),
            Some(AgentId::new("a1").unwrap()),
            DurationMs::from_millis(250),
        ))
        .await
        .unwrap();
    messaging
        .publish_task_failed(
            &TaskFailedEvent::new(
                envelope(),
                TaskId::new("t1").unwrap(),
                Specialty::new("triage").unwrap(),
                "validator.timeout",
                "deadline exceeded",
            )
            .unwrap(),
        )
        .await
        .unwrap();
    messaging
        .publish_deliberation_completed(&DeliberationCompletedEvent::new(
            envelope(),
            TaskId::new("t1").unwrap(),
            Specialty::new("triage").unwrap(),
            ProposalId::new("p1").unwrap(),
            Score::new(0.75).unwrap(),
            3,
            DurationMs::from_millis(900),
        ))
        .await
        .unwrap();
    messaging
        .publish_phase_changed(
            &PhaseChangedEvent::new(
                envelope(),
                TaskId::new("t1").unwrap(),
                "proposing",
                "revising",
            )
            .unwrap(),
        )
        .await
        .unwrap();

    // Collect — each subject must yield exactly one message.
    let got_dispatched = tokio::time::timeout(Duration::from_secs(5), sub_dispatched.next())
        .await
        .expect("timed out waiting for task.dispatched")
        .expect("subscription closed early");
    let got_completed = tokio::time::timeout(Duration::from_secs(5), sub_completed.next())
        .await
        .unwrap()
        .unwrap();
    let got_failed = tokio::time::timeout(Duration::from_secs(5), sub_failed.next())
        .await
        .unwrap()
        .unwrap();
    let got_deliberation = tokio::time::timeout(Duration::from_secs(5), sub_deliberation.next())
        .await
        .unwrap()
        .unwrap();
    let got_phase = tokio::time::timeout(Duration::from_secs(5), sub_phase.next())
        .await
        .unwrap()
        .unwrap();

    // Verify each payload parses as JSON with flat envelope fields
    // (AsyncAPI shape) and the correct event-specific fields.
    for (subject, payload) in [
        (&subjects.task_dispatched, got_dispatched.payload.clone()),
        (&subjects.task_completed, got_completed.payload.clone()),
        (&subjects.task_failed, got_failed.payload.clone()),
        (
            &subjects.deliberation_completed,
            got_deliberation.payload.clone(),
        ),
        (&subjects.phase_changed, got_phase.payload.clone()),
    ] {
        let v: serde_json::Value =
            serde_json::from_slice(&payload).unwrap_or_else(|e| panic!("{subject}: {e}"));
        let obj = v.as_object().unwrap();
        assert!(
            obj.contains_key("event_id"),
            "{subject}: missing event_id at root"
        );
        assert!(
            obj.contains_key("emitted_at"),
            "{subject}: missing emitted_at at root"
        );
        assert!(
            obj.contains_key("source"),
            "{subject}: missing source at root"
        );
        assert!(
            !obj.contains_key("envelope"),
            "{subject}: envelope must be flattened, not nested"
        );
    }

    // Sanity checks on event-specific fields.
    let completed: serde_json::Value = serde_json::from_slice(&got_completed.payload).unwrap();
    assert_eq!(completed["task_id"], "t1");
    assert_eq!(completed["specialty"], "triage");

    let failed: serde_json::Value = serde_json::from_slice(&got_failed.payload).unwrap();
    assert_eq!(failed["error_kind"], "validator.timeout");
    assert_eq!(failed["error_reason"], "deadline exceeded");
}
