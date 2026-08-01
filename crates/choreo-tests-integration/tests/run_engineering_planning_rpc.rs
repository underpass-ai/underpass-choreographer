use std::collections::BTreeMap;

use choreo_proto::v1::choreographer_service_client::ChoreographerServiceClient;
use choreo_proto::v1::RunCeremonyRequest;
use choreo_tests_integration::grpc_fixture::GrpcFixture;
use prost_types::{value::Kind, Struct, Value};

const ENGINEERING_PLANNING_CEREMONY: &str =
    include_str!("../../../tests/e2e/ceremonies/engineering-planning.yaml");

fn brief_context(brief: &str) -> Struct {
    Struct {
        fields: BTreeMap::from([(
            "mission_brief".to_owned(),
            Value {
                kind: Some(Kind::StringValue(brief.to_owned())),
            },
        )]),
    }
}

/// Drives the generic engineering-planning ceremony end-to-end, supplying
/// the mission only through the run-time `context` brief. Proves a
/// product-agnostic ceremony can be steered onto any mission without a
/// bespoke YAML — the handler renders the brief into each agent's task.
#[tokio::test]
async fn run_engineering_planning_executes_with_a_context_brief() {
    let fixture = GrpcFixture::start().await;
    let mut client = ChoreographerServiceClient::new(fixture.channel);

    let response = client
        .run_ceremony(RunCeremonyRequest {
            actor_id: "operator-1".to_owned(),
            actor_kind: "service".to_owned(),
            ceremony_id: "integration-engineering-planning".to_owned(),
            definition_yaml: ENGINEERING_PLANNING_CEREMONY.to_owned(),
            context: Some(brief_context(
                "Build an autonomous drone that photographs trees showing signs of disease.",
            )),
            lease_owner_id: "integration-test".to_owned(),
            lease_ttl_ms: 60_000,
        })
        .await
        .expect("RunCeremony should succeed")
        .into_inner();

    assert_eq!(response.definition_name, "engineering_planning");
    assert_eq!(response.final_state, "CLOSED");
    assert!(response.completed);
    assert_eq!(response.steps.len(), 4);
    assert!(response.steps.iter().all(|step| step.status == "COMPLETED"));

    for role in [
        "MISSION_OWNER",
        "SYSTEMS_ARCHITECT",
        "SAFETY_REVIEWER",
        "PROGRAM_LEAD",
    ] {
        assert!(
            response.steps.iter().any(|step| step.role_id == role),
            "missing completed role {role}"
        );
    }

    let diagram = &response.mermaid_sequence;
    assert!(diagram.contains("sequenceDiagram"));
    assert!(diagram.contains("frame_mission [facilitation_prompt]"));
    assert!(diagram.contains("plan_committed -> CLOSED"));
}
