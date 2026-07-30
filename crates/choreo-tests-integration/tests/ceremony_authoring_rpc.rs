//! Authoring a ceremony over gRPC.
//!
//! Checking a draft, being told what is wrong with it, and putting a
//! version in the catalogue. The first two answer about the YAML in
//! the request and leave no trace; the third is what makes a version
//! something an instance can be bound to.

use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::{
    ExplainCeremonyDraftRequest, PublishCeremonyDefinitionRequest, StartPublishedCeremonyRequest,
    ValidateCeremonyDraftRequest,
};
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use tonic::Code;

const PUBLISHABLE_CEREMONY: &str = r#"
version: "1.0"
name: "authored_ceremony"
states:
  - id: OPEN
    initial: true
  - id: DONE
    terminal: true
transitions:
  - from: OPEN
    to: DONE
    trigger: finish
steps:
  - id: work
    state: OPEN
    handler: embedded_noop
roles:
  - id: FACILITATOR
    allowed_actions:
      - work
      - finish
"#;

/// The same ceremony and version, with a materially different graph.
const ALTERED_CEREMONY: &str = r#"
version: "1.0"
name: "authored_ceremony"
states:
  - id: OPEN
    initial: true
  - id: CANCELLED
    terminal: true
transitions:
  - from: OPEN
    to: CANCELLED
    trigger: finish
steps:
  - id: work
    state: OPEN
    handler: embedded_noop
roles:
  - id: FACILITATOR
    allowed_actions:
      - work
      - finish
"#;

/// Its only transition leads somewhere it never declared, and its one
/// role is allowed to run a step that does not exist.
const BROKEN_CEREMONY: &str = r#"
version: "1.0"
name: "broken_ceremony"
states:
  - id: DRAFTING
    initial: true
  - id: DONE
    terminal: true
transitions:
  - from: DRAFTING
    to: NOWHERE
    trigger: finish
steps: []
roles:
  - id: FACILITATOR
    allowed_actions:
      - missing_step
"#;

#[tokio::test]
async fn a_sound_draft_validates_and_a_broken_one_says_where_it_is_broken() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let sound = client
        .validate_ceremony_draft(ValidateCeremonyDraftRequest {
            definition_yaml: PUBLISHABLE_CEREMONY.to_owned(),
        })
        .await
        .expect("ValidateCeremonyDraft should succeed")
        .into_inner();

    assert_eq!(sound.ceremony, "authored_ceremony");
    assert_eq!(sound.version, "1.0");
    assert!(sound.publishable);
    assert_eq!(sound.error_count, 0);

    let broken = client
        .validate_ceremony_draft(ValidateCeremonyDraftRequest {
            definition_yaml: BROKEN_CEREMONY.to_owned(),
        })
        .await
        .expect("validating a broken draft is an answer, not an error")
        .into_inner();

    assert!(!broken.publishable);
    assert_eq!(broken.error_count, 2);
    assert_eq!(broken.findings.len(), 2);
    assert!(broken
        .findings
        .iter()
        .all(|finding| finding.severity == "error"));

    // The locus names the element at fault with the same structure the
    // in-process surface carries, so one client can read either.
    let transition = broken
        .findings
        .iter()
        .find_map(|finding| finding.locus.as_ref())
        .expect("a finding should point at an element");
    assert!(transition.fields.contains_key("kind"));
}

#[tokio::test]
async fn a_draft_that_cannot_be_parsed_is_an_error_not_an_empty_report() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let status = client
        .validate_ceremony_draft(ValidateCeremonyDraftRequest {
            definition_yaml: "this is not a ceremony".to_owned(),
        })
        .await
        .expect_err("unparseable YAML has no findings to report");

    assert_ne!(status.code(), Code::Unknown);
}

#[tokio::test]
async fn explaining_a_draft_says_what_it_declares_and_what_blocks_it() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let explanation = client
        .explain_ceremony_draft(ExplainCeremonyDraftRequest {
            definition_yaml: BROKEN_CEREMONY.to_owned(),
        })
        .await
        .expect("ExplainCeremonyDraft should succeed")
        .into_inner();

    assert!(!explanation.publishable);
    let summary = explanation.summary.expect("an explanation should count");
    assert_eq!(summary.states, 2);
    assert_eq!(summary.initial_states, 1);
    assert_eq!(summary.terminal_states, 1);
    assert_eq!(summary.transitions, 1);

    assert!(explanation.narrative[0].contains("`broken_ceremony` declares 2 states"));
    assert!(explanation
        .narrative
        .iter()
        .any(|line| line.contains("block publication")));
    // Each defect names its element in words. A serialized object
    // quoted into the middle of a sentence is prose only by accident.
    assert!(explanation
        .narrative
        .iter()
        .any(|line| line.contains(" — at ") && !line.contains('{')));
}

#[tokio::test]
async fn publishing_is_idempotent_for_the_same_content_and_refuses_different_content() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let published = client
        .publish_ceremony_definition(PublishCeremonyDefinitionRequest {
            definition_yaml: PUBLISHABLE_CEREMONY.to_owned(),
        })
        .await
        .expect("PublishCeremonyDefinition should succeed")
        .into_inner();

    assert_eq!(published.outcome, "published");
    assert_eq!(published.ceremony, "authored_ceremony");
    assert_eq!(published.version, "1.0");
    assert!(!published.digest.is_empty());

    // The same content again is not a failure: an author retrying a
    // call they never saw answered must not be told they broke
    // something.
    let again = client
        .publish_ceremony_definition(PublishCeremonyDefinitionRequest {
            definition_yaml: PUBLISHABLE_CEREMONY.to_owned(),
        })
        .await
        .expect("republishing identical content should succeed")
        .into_inner();

    assert_eq!(again.outcome, "already_published");
    assert_eq!(again.digest, published.digest);

    // Different content under the same version is refused, and both
    // digests come back so the caller can see what differed rather
    // than being told only that something did.
    let occupied = client
        .publish_ceremony_definition(PublishCeremonyDefinitionRequest {
            definition_yaml: ALTERED_CEREMONY.to_owned(),
        })
        .await
        .expect("a refused publication is an answer, not a transport error")
        .into_inner();

    assert_eq!(occupied.outcome, "version_occupied");
    assert_eq!(occupied.published_digest, published.digest);
    assert_ne!(occupied.offered_digest, published.digest);
}

#[tokio::test]
async fn a_published_version_is_something_an_instance_can_be_started_from() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let published = client
        .publish_ceremony_definition(PublishCeremonyDefinitionRequest {
            definition_yaml: PUBLISHABLE_CEREMONY.to_owned(),
        })
        .await
        .expect("PublishCeremonyDefinition should succeed")
        .into_inner();

    // The whole point of publishing: this is now a version a session
    // can be bound to, over the same connection, with no other step.
    let started = client
        .start_published_ceremony(StartPublishedCeremonyRequest {
            ceremony_id: "authored-session".to_owned(),
            ceremony: "authored_ceremony".to_owned(),
            version: "1.0".to_owned(),
            context: None,
        })
        .await
        .expect("StartPublishedCeremony should succeed")
        .into_inner()
        .instance
        .expect("a started ceremony must come back");

    assert_eq!(started.bound_definition_digest, published.digest);
    assert_eq!(started.current_state, "OPEN");
    assert_eq!(started.next_step_id, "work");
}
