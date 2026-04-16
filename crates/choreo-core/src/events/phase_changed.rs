//! [`PhaseChangedEvent`] — a task moved between domain phases.
//!
//! The phase labels are free-form strings because different domains
//! carry different lifecycles. The Choreographer does not enumerate
//! them.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::events::envelope::EventEnvelope;
use crate::value_objects::TaskId;

const MAX_PHASE_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseChangedEvent {
    #[serde(flatten)]
    envelope: EventEnvelope,
    task_id: TaskId,
    from_phase: String,
    to_phase: String,
}

impl PhaseChangedEvent {
    pub fn new(
        envelope: EventEnvelope,
        task_id: TaskId,
        from_phase: impl Into<String>,
        to_phase: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let from_phase_str: String = from_phase.into();
        let to_phase_str: String = to_phase.into();
        let from_phase = Self::validate(&from_phase_str, "phase_changed.from_phase")?;
        let to_phase = Self::validate(&to_phase_str, "phase_changed.to_phase")?;
        Ok(Self {
            envelope,
            task_id,
            from_phase,
            to_phase,
        })
    }

    fn validate(raw: &str, field: &'static str) -> Result<String, DomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField { field });
        }
        if trimmed.len() > MAX_PHASE_LEN {
            return Err(DomainError::FieldTooLong {
                field,
                actual: trimmed.len(),
                max: MAX_PHASE_LEN,
            });
        }
        Ok(trimmed.to_owned())
    }

    #[must_use]
    pub fn envelope(&self) -> &EventEnvelope {
        &self.envelope
    }
    #[must_use]
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }
    #[must_use]
    pub fn from_phase(&self) -> &str {
        &self.from_phase
    }
    #[must_use]
    pub fn to_phase(&self) -> &str {
        &self.to_phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::EventId;
    use time::macros::datetime;

    fn env() -> EventEnvelope {
        EventEnvelope::new(
            EventId::new("e").unwrap(),
            datetime!(2026-04-15 12:00:00 UTC),
            "s",
            None,
        )
        .unwrap()
    }

    #[test]
    fn empty_phase_is_rejected() {
        assert!(matches!(
            PhaseChangedEvent::new(env(), TaskId::new("t").unwrap(), "  ", "next").unwrap_err(),
            DomainError::EmptyField {
                field: "phase_changed.from_phase"
            }
        ));
    }

    #[test]
    fn arbitrary_phase_labels_are_accepted() {
        for (from_phase, to_phase) in [
            ("open", "triaged"),
            ("intake", "classification"),
            ("sourcing", "negotiation"),
        ] {
            PhaseChangedEvent::new(env(), TaskId::new("t").unwrap(), from_phase, to_phase).unwrap();
        }
    }

    #[test]
    fn overlong_phase_is_rejected() {
        let err = PhaseChangedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            "from",
            "x".repeat(MAX_PHASE_LEN + 1),
        )
        .unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }
}
