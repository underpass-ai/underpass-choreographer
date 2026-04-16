//! [`TaskCompletedEvent`] — an agent finished its work on a task.

use serde::{Deserialize, Serialize};

use crate::events::envelope::EventEnvelope;
use crate::value_objects::{AgentId, DurationMs, Specialty, TaskId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCompletedEvent {
    #[serde(flatten)]
    envelope: EventEnvelope,
    task_id: TaskId,
    specialty: Specialty,
    agent_id: Option<AgentId>,
    duration: DurationMs,
}

impl TaskCompletedEvent {
    #[must_use]
    pub fn new(
        envelope: EventEnvelope,
        task_id: TaskId,
        specialty: Specialty,
        agent_id: Option<AgentId>,
        duration: DurationMs,
    ) -> Self {
        Self {
            envelope,
            task_id,
            specialty,
            agent_id,
            duration,
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
    pub fn agent_id(&self) -> Option<&AgentId> {
        self.agent_id.as_ref()
    }
    #[must_use]
    pub fn duration(&self) -> DurationMs {
        self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value_objects::EventId;
    use time::macros::datetime;

    #[test]
    fn accessors_return_fields() {
        let env = EventEnvelope::new(
            EventId::new("e").unwrap(),
            datetime!(2026-04-15 12:00:00 UTC),
            "s",
            None,
        )
        .unwrap();
        let ev = TaskCompletedEvent::new(
            env,
            TaskId::new("t").unwrap(),
            Specialty::new("triage").unwrap(),
            Some(AgentId::new("a").unwrap()),
            DurationMs::from_millis(250),
        );
        assert_eq!(ev.duration().get(), 250);
        assert_eq!(ev.agent_id().unwrap().as_str(), "a");
    }
}
