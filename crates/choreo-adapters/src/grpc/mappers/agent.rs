//! Agent summary: domain → proto.
//!
//! The domain model does not hold a typed `AgentSummary`; agents are
//! opaque behind [`AgentPort`]. This adapter composes the proto
//! summary from an [`AgentId`] plus a `kind` string supplied by the
//! composition root (e.g. `"noop"`, `"vllm"`, `"anthropic"`,
//! `"rule"`).

use choreo_core::value_objects::{AgentId, Attributes, Specialty};
use choreo_proto::v1 as pb;

use super::attributes::attributes_to_struct;

/// Project domain fields into a proto `AgentSummary`.
///
/// Kept on the module surface so list/status handlers can project
/// agent info when they start resolving live handles; `CouncilSummary`
/// responses still carry an empty `agents` list today because the
/// council aggregate stores only ids.
#[allow(dead_code)]
#[must_use]
pub fn agent_summary_from(
    agent_id: &AgentId,
    specialty: &Specialty,
    kind: &str,
    attributes: &Attributes,
) -> pb::AgentSummary {
    pb::AgentSummary {
        agent_id: agent_id.as_str().to_owned(),
        specialty: specialty.as_str().to_owned(),
        kind: kind.to_owned(),
        attributes: Some(attributes_to_struct(attributes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fields_copy_through() {
        let pb = agent_summary_from(
            &AgentId::new("a1").unwrap(),
            &Specialty::new("triage").unwrap(),
            "vllm",
            &Attributes::empty(),
        );
        assert_eq!(pb.agent_id, "a1");
        assert_eq!(pb.specialty, "triage");
        assert_eq!(pb.kind, "vllm");
        assert!(pb.attributes.is_some());
    }
}
