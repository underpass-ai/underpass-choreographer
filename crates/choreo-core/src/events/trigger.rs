//! [`TriggerEvent`] — inbound domain event requesting one or more
//! deliberations.
//!
//! Domain-neutral: the event carries a free-form `kind`, a list of
//! specialties whose councils should run, and an opaque payload.
//! The Choreographer does not interpret `kind` or `payload`; they
//! are adapter / operator concerns.

use serde::{Deserialize, Serialize};

use crate::entities::TaskConstraints;
use crate::error::DomainError;
use crate::events::envelope::EventEnvelope;
use crate::value_objects::{Attributes, Specialty, TaskDescription};

const MAX_KIND_LEN: usize = 128;

/// An inbound event that fans out into deliberations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerEvent {
    #[serde(flatten)]
    envelope: EventEnvelope,
    kind: String,
    requested_specialties: Vec<Specialty>,
    task_description_template: Option<TaskDescription>,
    constraints: TaskConstraints,
    payload: Attributes,
}

impl TriggerEvent {
    /// Build a trigger event.
    ///
    /// Invariants:
    /// - `kind` must be non-empty after trimming and within length bounds.
    /// - `requested_specialties` must be non-empty and deduplicated by
    ///   the caller is *not* required — we dedupe here so downstream
    ///   dispatch cannot double-run a council.
    pub fn new(
        envelope: EventEnvelope,
        kind: impl Into<String>,
        requested_specialties: impl IntoIterator<Item = Specialty>,
        task_description_template: Option<TaskDescription>,
        constraints: TaskConstraints,
        payload: Attributes,
    ) -> Result<Self, DomainError> {
        let kind = kind.into();
        let trimmed = kind.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "trigger.kind",
            });
        }
        if trimmed.len() > MAX_KIND_LEN {
            return Err(DomainError::FieldTooLong {
                field: "trigger.kind",
                actual: trimmed.len(),
                max: MAX_KIND_LEN,
            });
        }

        let mut seen = std::collections::BTreeSet::new();
        let mut unique = Vec::new();
        for sp in requested_specialties {
            if seen.insert(sp.clone()) {
                unique.push(sp);
            }
        }
        if unique.is_empty() {
            return Err(DomainError::EmptyCollection {
                field: "trigger.requested_specialties",
            });
        }

        Ok(Self {
            envelope,
            kind: trimmed.to_owned(),
            requested_specialties: unique,
            task_description_template,
            constraints,
            payload,
        })
    }

    #[must_use]
    pub fn envelope(&self) -> &EventEnvelope {
        &self.envelope
    }
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    #[must_use]
    pub fn requested_specialties(&self) -> &[Specialty] {
        &self.requested_specialties
    }
    #[must_use]
    pub fn task_description_template(&self) -> Option<&TaskDescription> {
        self.task_description_template.as_ref()
    }
    #[must_use]
    pub fn constraints(&self) -> &TaskConstraints {
        &self.constraints
    }
    #[must_use]
    pub fn payload(&self) -> &Attributes {
        &self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::EventId;
    use time::macros::datetime;

    fn env() -> EventEnvelope {
        EventEnvelope::new(
            EventId::new("e1").unwrap(),
            datetime!(2026-04-15 12:00:00 UTC),
            "grafana",
            None,
        )
        .unwrap()
    }

    fn sp(s: &str) -> Specialty {
        Specialty::new(s).unwrap()
    }

    #[test]
    fn empty_kind_is_rejected() {
        let err = TriggerEvent::new(
            env(),
            "   ",
            vec![sp("triage")],
            None,
            TaskConstraints::default(),
            Attributes::empty(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "trigger.kind"
            }
        ));
    }

    #[test]
    fn overlong_kind_is_rejected() {
        let err = TriggerEvent::new(
            env(),
            "k".repeat(MAX_KIND_LEN + 1),
            vec![sp("triage")],
            None,
            TaskConstraints::default(),
            Attributes::empty(),
        )
        .unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn empty_specialty_list_is_rejected() {
        let err = TriggerEvent::new(
            env(),
            "alert.fired",
            Vec::<Specialty>::new(),
            None,
            TaskConstraints::default(),
            Attributes::empty(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyCollection {
                field: "trigger.requested_specialties"
            }
        ));
    }

    #[test]
    fn duplicate_specialties_are_deduplicated() {
        let ev = TriggerEvent::new(
            env(),
            "alert.fired",
            vec![sp("triage"), sp("triage"), sp("reviewer")],
            None,
            TaskConstraints::default(),
            Attributes::empty(),
        )
        .unwrap();
        assert_eq!(ev.requested_specialties().len(), 2);
    }

    #[test]
    fn json_shape_is_flat_per_asyncapi() {
        // Regression test: AsyncAPI declares TriggerEvent via allOf
        // composition with EventEnvelope, so the JSON on the wire has
        // envelope fields at the top level next to `kind`,
        // `requested_specialties`, etc. This test would fail if the
        // `#[serde(flatten)]` attribute on `envelope` regressed.
        let ev = TriggerEvent::new(
            env(),
            "alert.fired",
            vec![sp("triage")],
            None,
            TaskConstraints::default(),
            Attributes::empty(),
        )
        .unwrap();
        let json = serde_json::to_value(&ev).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("event_id"));
        assert!(obj.contains_key("source"));
        assert!(obj.contains_key("emitted_at"));
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("requested_specialties"));
        assert!(
            !obj.contains_key("envelope"),
            "envelope must flatten into the root"
        );
    }

    #[test]
    fn kind_is_free_form_across_domains() {
        for kind in [
            "alert.fired",
            "case.opened",
            "shipment.delayed",
            "protocol.deviation.detected",
            "claim.submitted",
        ] {
            TriggerEvent::new(
                env(),
                kind,
                vec![sp("x")],
                None,
                TaskConstraints::default(),
                Attributes::empty(),
            )
            .unwrap();
        }
    }
}
