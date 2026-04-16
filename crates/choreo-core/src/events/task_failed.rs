//! [`TaskFailedEvent`] — a task failed during deliberation or execution.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::events::envelope::EventEnvelope;
use crate::value_objects::{Specialty, TaskId};

const MAX_REASON_LEN: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFailedEvent {
    #[serde(flatten)]
    envelope: EventEnvelope,
    task_id: TaskId,
    specialty: Specialty,
    error_kind: String,
    error_reason: String,
}

impl TaskFailedEvent {
    pub fn new(
        envelope: EventEnvelope,
        task_id: TaskId,
        specialty: Specialty,
        error_kind: impl Into<String>,
        error_reason: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let kind = error_kind.into();
        if kind.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "task_failed.error_kind",
            });
        }
        let reason = error_reason.into();
        if reason.len() > MAX_REASON_LEN {
            return Err(DomainError::FieldTooLong {
                field: "task_failed.error_reason",
                actual: reason.len(),
                max: MAX_REASON_LEN,
            });
        }
        Ok(Self {
            envelope,
            task_id,
            specialty,
            error_kind: kind,
            error_reason: reason,
        })
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
    pub fn specialty(&self) -> &Specialty {
        &self.specialty
    }
    #[must_use]
    pub fn error_kind(&self) -> &str {
        &self.error_kind
    }
    #[must_use]
    pub fn error_reason(&self) -> &str {
        &self.error_reason
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
    fn empty_kind_is_rejected() {
        let err = TaskFailedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            "   ",
            "reason",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "task_failed.error_kind"
            }
        ));
    }

    #[test]
    fn overlong_reason_is_rejected() {
        let err = TaskFailedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            "kind",
            "x".repeat(MAX_REASON_LEN + 1),
        )
        .unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn well_formed_event_keeps_fields() {
        let ev = TaskFailedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            "validator.timeout",
            "deadline exceeded",
        )
        .unwrap();
        assert_eq!(ev.error_kind(), "validator.timeout");
        assert_eq!(ev.error_reason(), "deadline exceeded");
    }
}
