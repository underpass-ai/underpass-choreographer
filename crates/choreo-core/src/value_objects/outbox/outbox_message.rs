use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::EventId;

use super::OutboxSubject;

/// A message enqueued in the same transaction as the state it reports.
///
/// The outbox exists so publication cannot disagree with what was
/// persisted. Because delivery is at-least-once, the message carries
/// the originating event's identity: that is what lets a consumer
/// recognise a redelivery instead of acting twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxMessage {
    event_id: EventId,
    subject: OutboxSubject,
    payload: Value,
    #[serde(with = "time::serde::rfc3339")]
    enqueued_at: OffsetDateTime,
}

impl OutboxMessage {
    pub fn new(
        event_id: EventId,
        subject: OutboxSubject,
        payload: Value,
        enqueued_at: OffsetDateTime,
    ) -> Result<Self, DomainError> {
        if payload.is_null() {
            return Err(DomainError::EmptyField {
                field: "outbox_message.payload",
            });
        }
        Ok(Self {
            event_id,
            subject,
            payload,
            enqueued_at,
        })
    }

    /// Stable across redeliveries — the consumer's idempotency key.
    #[must_use]
    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    #[must_use]
    pub fn subject(&self) -> &OutboxSubject {
        &self.subject
    }

    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.payload
    }

    #[must_use]
    pub fn enqueued_at(&self) -> OffsetDateTime {
        self.enqueued_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    fn subject() -> OutboxSubject {
        OutboxSubject::new("choreo.ceremony.completed").unwrap()
    }

    #[test]
    fn a_null_payload_is_rejected() {
        assert!(matches!(
            OutboxMessage::new(
                EventId::new("e1").unwrap(),
                subject(),
                Value::Null,
                datetime!(2026-07-29 09:00:00 UTC),
            ),
            Err(DomainError::EmptyField {
                field: "outbox_message.payload"
            })
        ));
    }

    #[test]
    fn the_event_identity_survives_a_round_trip() {
        let message = OutboxMessage::new(
            EventId::new("e1").unwrap(),
            subject(),
            json!({ "ceremony_id": "c1" }),
            datetime!(2026-07-29 09:00:00 UTC),
        )
        .unwrap();

        let restored: OutboxMessage =
            serde_json::from_value(serde_json::to_value(&message).unwrap()).unwrap();

        assert_eq!(restored, message);
        assert_eq!(restored.event_id().as_str(), "e1");
    }
}
