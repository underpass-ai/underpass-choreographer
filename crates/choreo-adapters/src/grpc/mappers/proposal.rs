//! Proposal: domain → proto.

use choreo_core::entities::Proposal;
use choreo_proto::v1 as pb;

use super::attributes::attributes_to_struct;

#[must_use]
pub fn proposal_to_proto(p: &Proposal) -> pb::Proposal {
    pb::Proposal {
        proposal_id: p.id().as_str().to_owned(),
        author_agent_id: p.author().as_str().to_owned(),
        content: p.content().to_owned(),
        metadata: Some(attributes_to_struct(p.attributes())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{AgentId, Attributes, ProposalId, Specialty};
    use time::macros::datetime;

    #[test]
    fn fields_copy_through() {
        let p = Proposal::new(
            ProposalId::new("p1").unwrap(),
            AgentId::new("a1").unwrap(),
            Specialty::new("triage").unwrap(),
            "hello",
            Attributes::empty(),
            datetime!(2026-04-15 12:00:00 UTC),
        )
        .unwrap();
        let pb = proposal_to_proto(&p);
        assert_eq!(pb.proposal_id, "p1");
        assert_eq!(pb.author_agent_id, "a1");
        assert_eq!(pb.content, "hello");
        assert!(pb.metadata.is_some());
    }
}
