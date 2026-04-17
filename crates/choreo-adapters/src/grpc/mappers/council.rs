//! Council proto ↔ domain conversion helpers.

use choreo_core::entities::Council;
use choreo_core::error::DomainError;
use choreo_core::value_objects::{AgentId, CouncilId, Specialty};
use choreo_proto::v1 as pb;
use time::OffsetDateTime;
use uuid::Uuid;

use super::timestamp::offset_to_timestamp;

/// Build a [`Council`] from a `CreateCouncilRequest`.
///
/// The proto request does not carry an id; we mint one here so the
/// transport surface stays minimal (clients would otherwise have to
/// generate UUIDs just for bookkeeping).
///
/// Not called by `service.rs` directly: that handler passes the same
/// information to `CreateCouncilUseCase::execute` so the use case
/// owns id minting. Kept as a public mapper so a future handler that
/// bypasses the use case (e.g. bulk-import tooling) can reuse the
/// exact same conversion.
#[allow(dead_code)]
pub fn council_from_create_request(
    req: &pb::CreateCouncilRequest,
    agent_ids: Vec<AgentId>,
    now: OffsetDateTime,
) -> Result<Council, DomainError> {
    let council_id = CouncilId::new(Uuid::new_v4().to_string())?;
    let specialty = Specialty::new(&req.specialty)?;
    Council::new(council_id, specialty, agent_ids, now)
}

/// Render a [`Council`] as a proto summary. If `agents` is supplied,
/// the summary carries a populated list; otherwise the `agents` field
/// stays empty (matches the `include_agents` wire option).
#[must_use]
pub fn council_summary_from(
    council: &Council,
    agents: Vec<pb::AgentSummary>,
) -> pb::CouncilSummary {
    pb::CouncilSummary {
        specialty: council.specialty().as_str().to_owned(),
        num_agents: u32::try_from(council.size()).unwrap_or(u32::MAX),
        agents,
        created_at: Some(offset_to_timestamp(council.created_at())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn req(specialty: &str) -> pb::CreateCouncilRequest {
        pb::CreateCouncilRequest {
            specialty: specialty.to_owned(),
            num_agents: 1,
            agent_config: None,
        }
    }

    #[test]
    fn council_is_constructed_with_minted_id() {
        let c = council_from_create_request(
            &req("triage"),
            vec![AgentId::new("a1").unwrap()],
            datetime!(2026-04-15 12:00:00 UTC),
        )
        .unwrap();
        assert_eq!(c.specialty().as_str(), "triage");
        assert!(!c.id().as_str().is_empty());
    }

    #[test]
    fn empty_specialty_is_rejected() {
        let err = council_from_create_request(
            &req("  "),
            vec![AgentId::new("a1").unwrap()],
            datetime!(2026-04-15 12:00:00 UTC),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField { field: "specialty" }
        ));
    }

    #[test]
    fn council_summary_serializes_size_and_timestamp() {
        let council = Council::new(
            CouncilId::new("c").unwrap(),
            Specialty::new("triage").unwrap(),
            vec![AgentId::new("a1").unwrap(), AgentId::new("a2").unwrap()],
            datetime!(2026-04-15 12:00:00 UTC),
        )
        .unwrap();
        let summary = council_summary_from(&council, vec![]);
        assert_eq!(summary.specialty, "triage");
        assert_eq!(summary.num_agents, 2);
        assert!(summary.created_at.is_some());
        assert!(summary.agents.is_empty());
    }
}
