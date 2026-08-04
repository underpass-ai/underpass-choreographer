//! The embedded engine behind the published contract.
//!
//! Everything here goes through [`CeremonyEngineApi`] and the plain types of
//! `choreo-api` — the way a consuming product sees the engine. If a test in
//! this file needs a `choreo-core` type to *assert* something, the contract is
//! leaking; core types appear only to arrange the scene.

use choreo_adapters::yaml::CeremonyDefinitionYaml;
use choreo_api::{ApiError, CeremonyEngineApi, CONTRACT_VERSION};
use choreo_app::usecases::RunCeremonyInput;
use choreo_core::entities::CeremonyDefinition;
use choreo_core::value_objects::{
    AuditActorKind, CeremonyContext, CeremonyId, DurationMs, LeaseOwnerId,
};
use choreo_embedded::{EmbeddedChoreographer, VERSION};

const LINEAR_CEREMONY: &str = r#"
version: "1.0"
name: "api_linear"
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
    handler: embedded_noop
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

fn run_input(ceremony_id: &str, definition: CeremonyDefinition) -> RunCeremonyInput {
    RunCeremonyInput::new(
        CeremonyId::new(ceremony_id).unwrap(),
        definition,
        CeremonyContext::empty(),
        LeaseOwnerId::new("api-host").unwrap(),
        DurationMs::from_millis(30_000),
        "operator-1",
        AuditActorKind::Service,
    )
}

async fn engine_with_one_ceremony() -> EmbeddedChoreographer {
    let embedded = EmbeddedChoreographer::default();
    let definition = CeremonyDefinitionYaml::parse_str(LINEAR_CEREMONY).unwrap();
    embedded.run(run_input("api-c1", definition)).await.unwrap();
    embedded
}

#[tokio::test]
async fn the_report_names_the_contract_the_release_and_every_method() {
    let embedded = EmbeddedChoreographer::default();
    let report = embedded.capabilities();

    assert_eq!(report.contract_version(), CONTRACT_VERSION);
    assert_eq!(report.library_version(), VERSION);
    assert!(report.supports("list_ceremonies"));
    assert!(
        report.supports("get_ceremony"),
        "a method that exists but is not declared cannot be checked at startup"
    );
}

#[tokio::test]
async fn a_consumer_reads_ceremonies_without_a_domain_type_in_sight() {
    let embedded = engine_with_one_ceremony().await;

    let ceremonies = embedded.ceremonies().await.unwrap();
    assert_eq!(ceremonies.len(), 1);

    let summary = &ceremonies[0];
    assert_eq!(summary.ceremony_id, "api-c1");
    assert_eq!(summary.definition_name, "api_linear");
    assert_eq!(summary.definition_version, "1.0");
    assert_eq!(summary.current_state, "COMPLETED");
    assert!(
        summary.completed_at_millis.is_some(),
        "a finished ceremony must read as finished"
    );
    assert!(
        summary.created_at_millis > 0,
        "instants travel as unix millis, not as a formatting of somebody's \
         time type"
    );
}

#[tokio::test]
async fn one_ceremony_is_fetched_by_the_identity_the_listing_gave() {
    let embedded = engine_with_one_ceremony().await;

    let listed = embedded.ceremonies().await.unwrap();
    let fetched = embedded.ceremony(&listed[0].ceremony_id).await.unwrap();

    assert_eq!(
        fetched, listed[0],
        "the listing and the fetch must describe the same ceremony the same way"
    );
}

#[tokio::test]
async fn an_unknown_ceremony_is_not_found_and_not_worth_retrying() {
    let embedded = EmbeddedChoreographer::default();

    let error = embedded.ceremony("api-missing").await.unwrap_err();
    assert!(matches!(
        &error,
        ApiError::CeremonyNotFound { ceremony_id } if ceremony_id == "api-missing"
    ));
    assert!(
        !error.is_transient(),
        "retrying will not make the ceremony exist"
    );
}

#[tokio::test]
async fn a_hostile_identity_is_refused_rather_than_looked_up() {
    let embedded = EmbeddedChoreographer::default();

    let error = embedded.ceremony("").await.unwrap_err();
    assert!(
        matches!(error, ApiError::Refused { .. }),
        "an identity the domain rejects never reaches storage: {error}"
    );
}

#[tokio::test]
async fn an_instance_from_an_unpublished_draft_carries_no_digest() {
    let embedded = engine_with_one_ceremony().await;

    let ceremonies = embedded.ceremonies().await.unwrap();
    assert!(
        ceremonies[0].definition_digest.is_none(),
        "a digest is a claim that a published, immutable definition ran; a \
         draft run must not make it"
    );
}
