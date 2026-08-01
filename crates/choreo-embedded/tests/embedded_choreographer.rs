use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_app::usecases::{
    ApplyCeremonyTransitionInput, ApproveCeremonyGuardInput, RunCeremonyInput, StartCeremonyInput,
};
use choreo_core::entities::CeremonyDefinition;
use choreo_core::value_objects::{
    AuditActorKind, CeremonyContext, CeremonyId, DurationMs, GuardName, LeaseOwnerId, RoleId,
    StepOutput, StepResult, TransitionTrigger,
};
use choreo_embedded::{EmbeddedChoreographer, VERSION};

const LINEAR_CEREMONY: &str = r#"
version: "1.0"
name: "embedded_linear"
states:
  - id: STARTED
    initial: true
  - id: COMPLETED
    terminal: true
transitions:
  - from: STARTED
    to: COMPLETED
    trigger: finish
    guards:
      - work_completed
steps:
  - id: work
    state: STARTED
    handler: host_callback
guards:
  work_completed:
    type: automated
    check: "step_status:work:COMPLETED"
roles:
  - id: SYSTEM
    allowed_actions:
      - work
      - finish
"#;

const HUMAN_GUARD_CEREMONY: &str = r#"
version: "1.0"
name: "embedded_human_guard"
states:
  - id: WAITING
    initial: true
  - id: APPROVED
    terminal: true
transitions:
  - from: WAITING
    to: APPROVED
    trigger: approve
    guards:
      - human_approved
guards:
  human_approved:
    type: human
    check: manual_approval
roles:
  - id: APPROVER
    allowed_actions:
      - approve
"#;

fn input(definition: CeremonyDefinition) -> RunCeremonyInput {
    RunCeremonyInput::new(
        CeremonyId::new("embedded-run").unwrap(),
        definition,
        CeremonyContext::empty(),
        LeaseOwnerId::new("embedded-host").unwrap(),
        DurationMs::from_millis(30_000),
        "operator-1",
        AuditActorKind::Service,
    )
}

#[tokio::test]
async fn default_runtime_runs_without_external_services() {
    let embedded = EmbeddedChoreographer::default();
    let definition = CeremonyDefinitionYaml::parse_str(LINEAR_CEREMONY).unwrap();

    let output = embedded.run(input(definition)).await.unwrap();

    assert!(output.instance().is_completed(output.definition()));
    assert_eq!(embedded.version(), VERSION);
    assert_eq!(
        embedded
            .transcript(output.instance().id())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn host_callback_is_an_embeddable_step_adapter() {
    let called = Arc::new(AtomicBool::new(false));
    let observed = called.clone();
    let embedded = EmbeddedChoreographer::builder()
        .with_step_handler_callback(move |_request| {
            let observed = observed.clone();
            async move {
                observed.store(true, Ordering::SeqCst);
                StepResult::completed(StepOutput::empty())
            }
        })
        .build();
    let definition = CeremonyDefinitionYaml::parse_str(LINEAR_CEREMONY).unwrap();

    let output = embedded.run(input(definition)).await.unwrap();

    assert!(called.load(Ordering::SeqCst));
    assert!(output.instance().is_completed(output.definition()));
}

#[tokio::test]
async fn mounting_yaml_uses_the_application_mount_use_case() {
    let embedded = EmbeddedChoreographer::default();

    let mounted = embedded.mount_yaml(LINEAR_CEREMONY).await.unwrap();

    assert_eq!(mounted.definitions().len(), 1);
    assert_eq!(embedded.definitions().await.unwrap().len(), 1);
}

#[tokio::test]
async fn host_can_drive_a_human_guard_incrementally() {
    let embedded = EmbeddedChoreographer::default();
    let mounted = embedded.mount_yaml(HUMAN_GUARD_CEREMONY).await.unwrap();
    let definition = mounted.definitions()[0].clone();
    let ceremony_id = CeremonyId::new("human-active-run").unwrap();

    embedded
        .start(StartCeremonyInput::new(
            ceremony_id.clone(),
            definition.name().clone(),
            definition.version().clone(),
            CeremonyContext::empty(),
            "operator-1",
            AuditActorKind::Service,
        ))
        .await
        .unwrap();
    embedded
        .approve_guard(ApproveCeremonyGuardInput::new(
            ceremony_id.clone(),
            GuardName::new("human_approved").unwrap(),
            RoleId::new("APPROVER").unwrap(),
            AuditActorKind::Human,
        ))
        .await
        .unwrap();
    let transitioned = embedded
        .apply_transition(ApplyCeremonyTransitionInput::new(
            ceremony_id.clone(),
            RoleId::new("APPROVER").unwrap(),
            AuditActorKind::Agent,
            TransitionTrigger::new("approve").unwrap(),
        ))
        .await
        .unwrap();

    assert!(transitioned.is_completed(&definition));
    assert!(embedded
        .instance(&ceremony_id)
        .await
        .unwrap()
        .is_completed(&definition));
}
