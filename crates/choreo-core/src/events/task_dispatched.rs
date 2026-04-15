//! [`TaskDispatchedEvent`] — published when the choreographer routes a
//! task to its council.

use serde::{Deserialize, Serialize};

use crate::events::envelope::EventEnvelope;
use crate::value_objects::{EventId, Specialty, TaskId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDispatchedEvent {
    envelope: EventEnvelope,
    task_id: TaskId,
    specialty: Specialty,
    trigger_event_id: Option<EventId>,
}

impl TaskDispatchedEvent {
    #[must_use]
    pub fn new(
        envelope: EventEnvelope,
        task_id: TaskId,
        specialty: Specialty,
        trigger_event_id: Option<EventId>,
    ) -> Self {
        Self {
            envelope,
            task_id,
            specialty,
            trigger_event_id,
        }
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
    pub fn trigger_event_id(&self) -> Option<&EventId> {
        self.trigger_event_id.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn env() -> EventEnvelope {
        EventEnvelope::new(
            EventId::new("e1").unwrap(),
            datetime!(2026-04-15 12:00:00 UTC),
            "choreographer",
            None,
        )
        .unwrap()
    }

    #[test]
    fn accessors_return_fields() {
        let ev = TaskDispatchedEvent::new(
            env(),
            TaskId::new("t1").unwrap(),
            Specialty::new("triage").unwrap(),
            Some(EventId::new("trig").unwrap()),
        );
        assert_eq!(ev.task_id().as_str(), "t1");
        assert_eq!(ev.specialty().as_str(), "triage");
        assert_eq!(ev.trigger_event_id().unwrap().as_str(), "trig");
        assert_eq!(ev.envelope().source(), "choreographer");
    }

    #[test]
    fn trigger_event_id_may_be_absent() {
        let ev = TaskDispatchedEvent::new(
            env(),
            TaskId::new("t").unwrap(),
            Specialty::new("s").unwrap(),
            None,
        );
        assert!(ev.trigger_event_id().is_none());
    }
}
