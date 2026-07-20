//! Integration test for YAML-backed ceremony mounting and application flow.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use choreo_adapters::clock::SystemClock;
use choreo_adapters::memory::{
    InMemoryCeremonyContextStore, InMemoryCeremonyDefinitionRepository,
    InMemoryCeremonyInstanceRepository,
};
use choreo_adapters::noop::NoopCeremonyStepHandler;
use choreo_adapters::yaml::FileSystemCeremonyDefinitionSource;
use choreo_app::usecases::{
    ApplyCeremonyTransitionInput, ApplyCeremonyTransitionUseCase, MountCeremonyDefinitionsUseCase,
    RunCeremonyStepInput, RunCeremonyStepUseCase, StartCeremonyInput, StartCeremonyUseCase,
};
use choreo_core::ports::CeremonyDefinitionRepositoryPort;
use choreo_core::value_objects::{
    CeremonyContext, CeremonyId, CeremonyName, CeremonyVersion, DurationMs, IdempotencyKey,
    LeaseOwnerId, RoleId, StateId, StepAttempt, StepId, StepStatus, TransitionTrigger,
};

const FOUR_PERSON_CEREMONY: &str = r#"
version: "1.0"
name: "editorial_meeting"
description: "A four-person editorial meeting ceremony"
inputs:
  required:
    - brief
  optional:
    - prior_notes
outputs:
  synthesis:
    type: object
states:
  - id: COLLECTING_VOICES
    initial: true
    terminal: false
  - id: COMPLETED
    initial: false
    terminal: true
transitions:
  - from: COLLECTING_VOICES
    to: COMPLETED
    trigger: "meeting_done"
    guards:
      - roundtable_completed
steps:
  - id: roundtable
    state: COLLECTING_VOICES
    handler: multiagent_round
    config:
      participants:
        - facilitator
        - historian
        - critic
        - synthesizer
      prompt: "Discuss the brief and produce one synthesis."
guards:
  roundtable_completed:
    type: automated
    check: "step_status:roundtable:COMPLETED"
roles:
  - id: FACILITATOR
    allowed_actions:
      - roundtable
      - meeting_done
timeouts:
  step_default: 60
retry_policies:
  default:
    max_attempts: 2
    backoff_seconds: 1
"#;

#[tokio::test]
async fn yaml_definition_can_drive_the_application_ceremony_flow() {
    let fixture_dir = create_fixture_directory();
    let fixture_path = fixture_dir.join("editorial_meeting.yaml");
    fs::write(&fixture_path, FOUR_PERSON_CEREMONY).unwrap();

    let source =
        Arc::new(FileSystemCeremonyDefinitionSource::from_directory(&fixture_dir).unwrap());
    let definitions = Arc::new(InMemoryCeremonyDefinitionRepository::new());
    let instances = Arc::new(InMemoryCeremonyInstanceRepository::new());
    let context_store = Arc::new(InMemoryCeremonyContextStore::new());
    let handler = Arc::new(NoopCeremonyStepHandler::new());
    let clock = Arc::new(SystemClock::new());

    let mount = MountCeremonyDefinitionsUseCase::new(source, definitions.clone());
    let mounted = mount.execute().await.unwrap();
    assert_eq!(mounted.definitions().len(), 1);

    let definition_name = CeremonyName::new("editorial_meeting").unwrap();
    let definition_version = CeremonyVersion::v1();
    let definition = definitions
        .get(&definition_name, &definition_version)
        .await
        .unwrap();

    let start = StartCeremonyUseCase::new(definitions.clone(), instances.clone(), clock.clone());
    let instance = start
        .execute(StartCeremonyInput::new(
            CeremonyId::new("meeting-1").unwrap(),
            definition_name.clone(),
            definition_version.clone(),
            CeremonyContext::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(
        instance.current_state(),
        &StateId::new("COLLECTING_VOICES").unwrap()
    );

    let run_step = RunCeremonyStepUseCase::new(
        definitions.clone(),
        instances.clone(),
        handler,
        clock.clone(),
    )
    .with_context_store(context_store);
    let step_output = run_step
        .execute(RunCeremonyStepInput::new(
            CeremonyId::new("meeting-1").unwrap(),
            definition_name.clone(),
            definition_version.clone(),
            RoleId::new("FACILITATOR").unwrap(),
            StepId::new("roundtable").unwrap(),
            LeaseOwnerId::new("runner-1").unwrap(),
            IdempotencyKey::new("meeting-1-roundtable-1").unwrap(),
            DurationMs::from_millis(60_000),
        ))
        .await
        .unwrap();
    assert_eq!(step_output.attempt(), StepAttempt::FIRST);
    assert_eq!(step_output.result().status(), StepStatus::Completed);

    let transition = ApplyCeremonyTransitionUseCase::new(definitions, instances, clock);
    let completed = transition
        .execute(ApplyCeremonyTransitionInput::new(
            CeremonyId::new("meeting-1").unwrap(),
            definition_name,
            definition_version,
            RoleId::new("FACILITATOR").unwrap(),
            TransitionTrigger::new("meeting_done").unwrap(),
        ))
        .await
        .unwrap();

    assert!(completed.is_completed(&definition));
    assert_eq!(
        completed.current_state(),
        &StateId::new("COMPLETED").unwrap()
    );

    fs::remove_dir_all(fixture_dir).unwrap();
}

fn create_fixture_directory() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "choreo-ceremony-yaml-{}-{}",
        std::process::id(),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
