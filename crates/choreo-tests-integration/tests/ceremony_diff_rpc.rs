//! Comparing two ceremony definitions over gRPC.
//!
//! An author's question is rarely "what are the differences" — both
//! documents are in front of them. It is "may I adopt this while a
//! meeting is happening", and that is what the answer leads with.

use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    CeremonyDefinitionRef, DiffCeremonyDefinitionsRequest, PublishCeremonyDefinitionRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use tonic::Code;

const PUBLISHED: &str = r#"
version: "1.0"
name: "diffed_ceremony"
states:
  - id: OPEN
    initial: true
  - id: REVIEW
  - id: DONE
    terminal: true
transitions:
  - from: OPEN
    to: REVIEW
    trigger: opened
  - from: REVIEW
    to: DONE
    trigger: finish
steps:
  - id: work
    state: OPEN
    handler: noop
roles:
  - id: FACILITATOR
    allowed_actions:
      - work
      - opened
      - finish
"#;

/// The same ceremony with the middle state gone.
const WITHOUT_REVIEW: &str = r#"
version: "1.1"
name: "diffed_ceremony"
states:
  - id: OPEN
    initial: true
  - id: DONE
    terminal: true
transitions:
  - from: OPEN
    to: DONE
    trigger: opened
steps:
  - id: work
    state: OPEN
    handler: noop
roles:
  - id: FACILITATOR
    allowed_actions:
      - work
      - opened
"#;

/// The same ceremony with one more role and nothing taken away.
const WITH_OBSERVER: &str = r#"
version: "1.1"
name: "diffed_ceremony"
states:
  - id: OPEN
    initial: true
  - id: REVIEW
  - id: DONE
    terminal: true
transitions:
  - from: OPEN
    to: REVIEW
    trigger: opened
  - from: REVIEW
    to: DONE
    trigger: finish
steps:
  - id: work
    state: OPEN
    handler: noop
roles:
  - id: FACILITATOR
    allowed_actions:
      - work
      - opened
      - finish
  - id: OBSERVER
    allowed_actions:
      - request_intervention
"#;

fn supplied(yaml: &str) -> CeremonyDefinitionRef {
    CeremonyDefinitionRef {
        ceremony: String::new(),
        version: String::new(),
        definition_yaml: yaml.to_owned(),
    }
}

fn published(version: &str) -> CeremonyDefinitionRef {
    CeremonyDefinitionRef {
        ceremony: "diffed_ceremony".to_owned(),
        version: version.to_owned(),
        definition_yaml: String::new(),
    }
}

#[tokio::test]
async fn a_definition_does_not_differ_from_itself() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let diff = client
        .diff_ceremony_definitions(DiffCeremonyDefinitionsRequest {
            before: Some(supplied(PUBLISHED)),
            after: Some(supplied(PUBLISHED)),
        })
        .await
        .expect("DiffCeremonyDefinitions should succeed")
        .into_inner();

    assert!(diff.identical);
    assert!(!diff.strands_running_sessions);
    assert_eq!(diff.strand_count, 0);
    assert!(diff.changes.is_empty());
}

#[tokio::test]
async fn taking_a_state_away_is_reported_as_stranding_and_says_which_state() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let diff = client
        .diff_ceremony_definitions(DiffCeremonyDefinitionsRequest {
            before: Some(supplied(PUBLISHED)),
            after: Some(supplied(WITHOUT_REVIEW)),
        })
        .await
        .expect("DiffCeremonyDefinitions should succeed")
        .into_inner();

    assert!(!diff.identical);
    assert!(diff.strands_running_sessions);
    assert!(diff.strand_count > 0);

    // The locus names the element in the same structure every other
    // finding on this surface uses.
    let removed_state = diff
        .changes
        .iter()
        .find(|change| {
            change.kind == "removed"
                && change
                    .locus
                    .as_ref()
                    .and_then(|locus| locus.fields.get("state"))
                    .is_some()
        })
        .expect("the state that went away should be named");
    assert_eq!(removed_state.impact, "strands");
    assert!(!removed_state.detail.is_empty());
}

#[tokio::test]
async fn adding_a_role_changes_the_definition_without_stranding_anyone() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let diff = client
        .diff_ceremony_definitions(DiffCeremonyDefinitionsRequest {
            before: Some(supplied(PUBLISHED)),
            after: Some(supplied(WITH_OBSERVER)),
        })
        .await
        .expect("DiffCeremonyDefinitions should succeed")
        .into_inner();

    assert!(!diff.identical);
    assert!(
        !diff.strands_running_sessions,
        "another role at the table takes nothing away from a session already under way"
    );
    assert_eq!(diff.strand_count, 0);
    assert!(diff.changes.iter().all(|change| change.impact == "carries"));
}

#[tokio::test]
async fn what_is_published_can_be_compared_against_what_is_about_to_be() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    client
        .publish_ceremony_definition(PublishCeremonyDefinitionRequest {
            definition_yaml: PUBLISHED.to_owned(),
        })
        .await
        .expect("PublishCeremonyDefinition should succeed");

    // The question an author actually asks: is the next version safe
    // to adopt while sessions are running on this one?
    let diff = client
        .diff_ceremony_definitions(DiffCeremonyDefinitionsRequest {
            before: Some(published("1.0")),
            after: Some(supplied(WITHOUT_REVIEW)),
        })
        .await
        .expect("comparing the catalogue against a draft should succeed")
        .into_inner();

    assert!(diff.strands_running_sessions);
}

#[tokio::test]
async fn a_version_that_was_never_published_is_not_found() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let status = client
        .diff_ceremony_definitions(DiffCeremonyDefinitionsRequest {
            before: Some(published("9.9")),
            after: Some(supplied(PUBLISHED)),
        })
        .await
        .expect_err("a version nobody published cannot be compared against");

    assert_eq!(status.code(), Code::NotFound);
}

#[tokio::test]
async fn naming_and_supplying_a_definition_at_once_is_refused() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    // Both together has no reading, and guessing which one the caller
    // meant would answer a question they did not ask.
    let status = client
        .diff_ceremony_definitions(DiffCeremonyDefinitionsRequest {
            before: Some(CeremonyDefinitionRef {
                ceremony: "diffed_ceremony".to_owned(),
                version: "1.0".to_owned(),
                definition_yaml: PUBLISHED.to_owned(),
            }),
            after: Some(supplied(PUBLISHED)),
        })
        .await
        .expect_err("a side is either published or supplied, not both");

    assert_ne!(status.code(), Code::Unknown);
}
