#![cfg(feature = "redb")]

use std::collections::BTreeMap;

use choreo_api::{CeremonyEngineApi, StartCeremonyRequest};
use choreo_embedded::EmbeddedChoreographer;

const DEFINITION: &str = r#"
version: "1.0"
name: "durable_public_contract"
states:
  - id: OPEN
    initial: true
  - id: CLOSED
    terminal: true
transitions:
  - from: OPEN
    to: CLOSED
    trigger: close
    guards: []
steps: []
guards: {}
roles: []
"#;

#[tokio::test]
async fn published_definition_and_instance_survive_reopening_via_the_public_surface() {
    let directory = tempfile::tempdir().expect("temporary state directory");
    let path = directory.path().join("choreographer.redb");

    let engine = EmbeddedChoreographer::open_redb(&path).expect("durable engine opens");
    let analysis = engine
        .analyze_definition(DEFINITION)
        .await
        .expect("definition analyzes");
    assert_eq!(analysis.definition_name, "durable_public_contract");
    assert_eq!(analysis.definition_version, "1.0");
    assert!(analysis.publishable);
    CeremonyEngineApi::publish_definition(&engine, DEFINITION)
        .await
        .expect("definition publishes");
    engine
        .start_ceremony(StartCeremonyRequest {
            ceremony_id: "ceremony-1".to_owned(),
            definition_name: analysis.definition_name,
            definition_version: analysis.definition_version,
            context: BTreeMap::new(),
            actor_id: "host-1".to_owned(),
            actor_kind: "service".to_owned(),
        })
        .await
        .expect("published ceremony starts");
    drop(engine);

    let reopened = EmbeddedChoreographer::open_redb(&path).expect("durable engine reopens");
    let ceremony = reopened
        .ceremony("ceremony-1")
        .await
        .expect("instance survives restart");
    assert_eq!(ceremony.definition_name, "durable_public_contract");
    assert_eq!(ceremony.definition_version, "1.0");
    assert!(ceremony.definition_digest.is_some());
}
