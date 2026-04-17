//! Integration test: inbound `TriggerEvent` over NATS drives the
//! full deliberation pipeline end-to-end.
//!
//! Exercises:
//!   1. `NatsTriggerSubscriber` consuming on the configured wildcard.
//!   2. `AutoDispatchService` fanning out to `DeliberateUseCase`.
//!   3. The aggregate running through Proposing → Revising →
//!      Validating → Scoring → Completed.
//!   4. `NatsMessaging` publishing `deliberation.completed` on the
//!      canonical outbound subject.
//!   5. `InMemoryDeliberationRepository` holding the final aggregate
//!      keyed by the task id the event names.
//!
//! Runs only when the `container-tests` feature is enabled (CI).

#![cfg(feature = "container-tests")]

use std::sync::Arc;
use std::time::Duration;

use choreo_adapters::clock::SystemClock;
use choreo_adapters::memory::{
    InMemoryAgentRegistry, InMemoryCouncilRegistry, InMemoryDeliberationRepository,
    InMemoryStatistics,
};
use choreo_adapters::nats::{NatsMessaging, NatsSubjects, NatsTriggerSubscriber};
use choreo_adapters::noop::NoopAgent;
use choreo_adapters::scoring::UniformScoring;
use choreo_adapters::validators::ContentNonEmptyValidator;
use choreo_app::services::AutoDispatchService;
use choreo_app::usecases::DeliberateUseCase;
use choreo_core::entities::{Council, DeliberationPhase};
use choreo_core::ports::{
    AgentPort, ClockPort, CouncilRegistryPort, DeliberationRepositoryPort, MessagingPort,
    ScoringPort, StatisticsPort, ValidatorPort,
};
use choreo_core::value_objects::{AgentId, CouncilId, Specialty, TaskId};
use futures::StreamExt;
use serde_json::json;
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const NATS_IMAGE: &str = "nats";
const NATS_TAG: &str = "2";

async fn start_nats() -> (
    async_nats::Client,
    testcontainers::ContainerAsync<GenericImage>,
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

    let mut last_err = None;
    for _ in 0..20 {
        match async_nats::connect(&url).await {
            Ok(client) => return (client, container),
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    panic!("could not connect to nats after warmup: {last_err:?}");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // a single integration scenario wires the full stack
async fn trigger_event_over_nats_drives_full_pipeline() {
    let (client, _container) = start_nats().await;

    let subjects = NatsSubjects::new("choreo", "choreo.trigger.>").unwrap();
    let specialty = Specialty::new("triage").unwrap();

    // --- Wire the application with NATS messaging on both directions.
    let clock: Arc<dyn ClockPort> = Arc::new(SystemClock::new());
    let registry = Arc::new(InMemoryCouncilRegistry::new());
    let agent_registry = Arc::new(InMemoryAgentRegistry::new());
    let repo = Arc::new(InMemoryDeliberationRepository::new());

    let agent_id = AgentId::new("int-agent").unwrap();
    let agent: Arc<dyn AgentPort> = Arc::new(NoopAgent::new(agent_id.clone(), specialty.clone()));
    agent_registry.insert(agent).await.unwrap();

    let council = Council::new(
        CouncilId::new("int-council").unwrap(),
        specialty.clone(),
        vec![agent_id],
        clock.now(),
    )
    .unwrap();
    registry.register(council).await.unwrap();

    let validators: Vec<Arc<dyn ValidatorPort>> = vec![Arc::new(ContentNonEmptyValidator::new())];
    let scoring: Arc<dyn ScoringPort> = Arc::new(UniformScoring::new());
    let messaging: Arc<dyn MessagingPort> =
        Arc::new(NatsMessaging::new(client.clone(), subjects.clone()));
    let statistics: Arc<dyn StatisticsPort> = Arc::new(InMemoryStatistics::new());

    let deliberate = Arc::new(DeliberateUseCase::new(
        clock.clone(),
        registry.clone(),
        agent_registry.clone(),
        validators,
        scoring,
        repo.clone(),
        messaging.clone(),
        statistics,
        "integration-test",
    ));

    let dispatch = Arc::new(
        AutoDispatchService::new(
            deliberate,
            "Integration test default task description (fallback).",
        )
        .unwrap(),
    );

    // --- Observe the outbound completion event on NATS.
    let mut completion_sub = client
        .subscribe(subjects.deliberation_completed.clone())
        .await
        .unwrap();
    client.flush().await.unwrap();

    // --- Spawn the inbound subscriber.
    let subscriber = NatsTriggerSubscriber::new(client.clone(), subjects.clone(), dispatch);
    let handle = subscriber.spawn().await.unwrap();

    // Small settle so the subscription is in place before we publish.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // --- Publish a TriggerEvent matching the AsyncAPI contract.
    let event_id = uuid::Uuid::new_v4().to_string();
    let emitted_at = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
    let payload = json!({
        "event_id": event_id,
        "kind": "alert.fired",
        "source": "integration-test",
        "emitted_at": emitted_at,
        "requested_specialties": ["triage"],
        "task_description_template": "Investigate the incoming alert.",
        "constraints": {},
        "payload": {
            "severity": "p1",
            "service": "payments-api"
        }
    });
    let body = serde_json::to_vec(&payload).unwrap();
    client
        .publish("choreo.trigger.alert.fired".to_owned(), body.into())
        .await
        .unwrap();
    client.flush().await.unwrap();

    // --- Wait for the completion event.
    let completion_msg = tokio::time::timeout(Duration::from_secs(15), completion_sub.next())
        .await
        .expect("timed out waiting for deliberation.completed")
        .expect("subscription closed early");

    let completion_json: serde_json::Value = serde_json::from_slice(&completion_msg.payload)
        .expect("valid json on deliberation.completed");
    let task_id_str = completion_json
        .get("task_id")
        .and_then(|v| v.as_str())
        .expect("task_id present at root of deliberation.completed");
    let event_specialty = completion_json
        .get("specialty")
        .and_then(|v| v.as_str())
        .expect("specialty present");
    assert_eq!(event_specialty, "triage");
    // Envelope fields at the root per AsyncAPI allOf composition.
    assert!(completion_json.get("event_id").is_some());
    assert!(completion_json.get("emitted_at").is_some());
    assert_eq!(
        completion_json.get("source").and_then(|v| v.as_str()),
        Some("integration-test")
    );
    // A winner must have been named and validated.
    let winner = completion_json
        .get("winner_proposal_id")
        .and_then(|v| v.as_str())
        .expect("winner_proposal_id present");
    assert!(!winner.is_empty());
    // The number of candidates equals the council size (1 agent → 1 proposal).
    assert_eq!(
        completion_json
            .get("num_candidates")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );

    // --- Cross-check: the same task id is persisted in the repository.
    let task_id = TaskId::new(task_id_str).expect("task_id from wire must validate");
    let deliberation = repo
        .get(&task_id)
        .await
        .expect("deliberation missing from repo despite completion event");
    assert_eq!(deliberation.specialty(), &specialty);
    assert_eq!(deliberation.phase(), DeliberationPhase::Completed);
    assert_eq!(deliberation.proposals().len(), 1);
    assert_eq!(deliberation.outcomes().len(), 1);

    handle.abort();
}
