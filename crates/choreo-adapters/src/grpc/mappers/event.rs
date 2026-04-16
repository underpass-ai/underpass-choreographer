//! TriggerEvent: proto → domain.

use choreo_core::entities::TaskConstraints;
use choreo_core::error::DomainError;
use choreo_core::events::{EventEnvelope, TriggerEvent};
use choreo_core::value_objects::{
    DurationMs, EventId, NumAgents, Rounds, Specialty, TaskDescription,
};
use choreo_proto::v1 as pb;
use prost_types::Timestamp;
use time::OffsetDateTime;
use uuid::Uuid;

use super::attributes::{attributes_from_struct, rubric_from_struct};

/// Convert a proto `TriggerEvent` to the domain aggregate, minting
/// a fresh `EventId` on the fly if the wire carried an empty one and
/// timestamping with "now" if the proto did not specify `emitted_at`.
pub fn trigger_event_from_proto(
    ev: pb::TriggerEvent,
    fallback_now: OffsetDateTime,
) -> Result<TriggerEvent, DomainError> {
    let event_id = if ev.event_id.trim().is_empty() {
        EventId::new(Uuid::new_v4().to_string())?
    } else {
        EventId::new(ev.event_id)?
    };
    let emitted_at = ev.emitted_at.map_or(fallback_now, timestamp_to_offset);
    let envelope = EventEnvelope::new(event_id, emitted_at, ev.source, None)?;

    let specialties: Vec<Specialty> = ev
        .requested_specialties
        .into_iter()
        .map(Specialty::new)
        .collect::<Result<_, _>>()?;

    let template = if ev.task_description_template.trim().is_empty() {
        None
    } else {
        Some(TaskDescription::new(ev.task_description_template)?)
    };

    let constraints = match ev.constraints {
        None => TaskConstraints::default(),
        Some(c) => TaskConstraints::new(
            rubric_from_struct(c.rubric)?,
            if c.rounds == 0 {
                Rounds::default()
            } else {
                Rounds::new(c.rounds)?
            },
            if c.num_agents == 0 {
                None
            } else {
                Some(NumAgents::new(c.num_agents)?)
            },
            if c.deadline_ms == 0 {
                None
            } else {
                Some(DurationMs::from_millis(c.deadline_ms))
            },
        ),
    };

    let payload = attributes_from_struct(ev.payload)?;

    TriggerEvent::new(
        envelope,
        ev.kind,
        specialties,
        template,
        constraints,
        payload,
    )
}

fn timestamp_to_offset(ts: Timestamp) -> OffsetDateTime {
    let nanos = i128::from(ts.seconds) * 1_000_000_000 + i128::from(ts.nanos);
    OffsetDateTime::from_unix_timestamp_nanos(nanos).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::Struct as PbStruct;
    use time::macros::datetime;

    fn now() -> OffsetDateTime {
        datetime!(2026-04-15 12:00:00 UTC)
    }

    fn proto_trigger(kind: &str, specialties: Vec<&str>) -> pb::TriggerEvent {
        pb::TriggerEvent {
            event_id: String::new(),
            kind: kind.to_owned(),
            source: "grafana".to_owned(),
            emitted_at: None,
            requested_specialties: specialties.into_iter().map(str::to_owned).collect(),
            task_description_template: String::new(),
            constraints: None,
            payload: Some(PbStruct::default()),
        }
    }

    #[test]
    fn well_formed_trigger_roundtrips_with_minted_event_id() {
        let ev = trigger_event_from_proto(
            proto_trigger("alert.fired", vec!["triage", "reviewer"]),
            now(),
        )
        .unwrap();
        assert_eq!(ev.kind(), "alert.fired");
        assert_eq!(ev.requested_specialties().len(), 2);
        assert_eq!(ev.envelope().source(), "grafana");
        assert_eq!(ev.envelope().emitted_at(), now());
        assert!(!ev.envelope().event_id().as_str().is_empty());
    }

    #[test]
    fn empty_specialties_list_is_rejected() {
        let err = trigger_event_from_proto(proto_trigger("k", vec![]), now()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyCollection {
                field: "trigger.requested_specialties"
            }
        ));
    }

    #[test]
    fn empty_kind_is_rejected() {
        let err = trigger_event_from_proto(proto_trigger("  ", vec!["t"]), now()).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "trigger.kind"
            }
        ));
    }

    #[test]
    fn supplied_emitted_at_is_preserved() {
        let mut t = proto_trigger("k", vec!["s"]);
        t.emitted_at = Some(Timestamp {
            seconds: 1_800_000_000,
            nanos: 0,
        });
        let ev = trigger_event_from_proto(t, now()).unwrap();
        assert_ne!(ev.envelope().emitted_at(), now());
    }

    #[test]
    fn blank_template_is_treated_as_absent() {
        let mut t = proto_trigger("k", vec!["s"]);
        t.task_description_template = "   ".to_owned();
        let ev = trigger_event_from_proto(t, now()).unwrap();
        assert!(ev.task_description_template().is_none());
    }
}
