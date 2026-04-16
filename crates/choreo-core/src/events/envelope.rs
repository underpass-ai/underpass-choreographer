//! Common metadata carried by every domain event.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::value_objects::EventId;

const MAX_SOURCE_LEN: usize = 256;

/// Shared header attached to every domain event produced or consumed
/// by the Choreographer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    event_id: EventId,
    #[serde(with = "time::serde::rfc3339")]
    emitted_at: OffsetDateTime,
    source: String,
    #[serde(default)]
    correlation_id: Option<EventId>,
}

impl EventEnvelope {
    pub fn new(
        event_id: EventId,
        emitted_at: OffsetDateTime,
        source: impl Into<String>,
        correlation_id: Option<EventId>,
    ) -> Result<Self, DomainError> {
        let source = source.into();
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "event.source",
            });
        }
        if trimmed.len() > MAX_SOURCE_LEN {
            return Err(DomainError::FieldTooLong {
                field: "event.source",
                actual: trimmed.len(),
                max: MAX_SOURCE_LEN,
            });
        }
        Ok(Self {
            event_id,
            emitted_at,
            source: trimmed.to_owned(),
            correlation_id,
        })
    }

    #[must_use]
    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }
    #[must_use]
    pub fn emitted_at(&self) -> OffsetDateTime {
        self.emitted_at
    }
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub fn correlation_id(&self) -> Option<&EventId> {
        self.correlation_id.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn at() -> OffsetDateTime {
        datetime!(2026-04-15 12:00:00 UTC)
    }

    #[test]
    fn construction_trims_source() {
        let env =
            EventEnvelope::new(EventId::new("e1").unwrap(), at(), "  grafana  ", None).unwrap();
        assert_eq!(env.source(), "grafana");
    }

    #[test]
    fn empty_source_is_rejected() {
        let err = EventEnvelope::new(EventId::new("e1").unwrap(), at(), "   ", None).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "event.source"
            }
        ));
    }

    #[test]
    fn overlong_source_is_rejected() {
        let err = EventEnvelope::new(
            EventId::new("e1").unwrap(),
            at(),
            "x".repeat(MAX_SOURCE_LEN + 1),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn correlation_id_is_optional() {
        let env = EventEnvelope::new(EventId::new("e").unwrap(), at(), "s", None).unwrap();
        assert!(env.correlation_id().is_none());
    }

    #[test]
    fn accessors_return_fields() {
        let corr = EventId::new("c").unwrap();
        let env = EventEnvelope::new(EventId::new("e").unwrap(), at(), "src", Some(corr.clone()))
            .unwrap();
        assert_eq!(env.event_id().as_str(), "e");
        assert_eq!(env.emitted_at(), at());
        assert_eq!(env.correlation_id(), Some(&corr));
    }

    #[test]
    fn json_shape_matches_asyncapi_allof() {
        // AsyncAPI composes EventEnvelope into each event via `allOf`,
        // which produces a flat JSON object. Events use
        // `#[serde(flatten)]` on their `envelope` field so the wire
        // shape matches this expectation. This test locks in the
        // EventEnvelope keys at the JSON root; breaking it is a
        // breaking change on the event bus contract.
        let env = EventEnvelope::new(
            EventId::new("e").unwrap(),
            at(),
            "src",
            Some(EventId::new("c").unwrap()),
        )
        .unwrap();
        let json = serde_json::to_value(&env).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("event_id"));
        assert!(obj.contains_key("emitted_at"));
        assert!(obj.contains_key("source"));
        assert!(obj.contains_key("correlation_id"));
    }
}
