//! NATS adapter configuration.
//!
//! Honest defaults matching the Helm chart's `values.yaml`:
//! - publish prefix: `choreo`
//! - inbound trigger subject wildcard: `choreo.trigger.>`
//! - outbound subjects rooted at `<prefix>.<event>`.

use choreo_core::error::DomainError;

const MAX_SUBJECT_LEN: usize = 256;

/// NATS connection and subject configuration.
///
/// Built from the service configuration (`ServiceConfig`) at wiring
/// time. Kept as a value object so the adapter can validate invariants
/// up-front without a dependency on the broker.
#[derive(Debug, Clone)]
pub struct NatsConfig {
    pub url: String,
    pub subjects: NatsSubjects,
}

impl NatsConfig {
    pub fn new(
        url: impl Into<String>,
        publish_prefix: impl Into<String>,
        trigger_subject: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let url = url.into();
        let url_trimmed = url.trim();
        if url_trimmed.is_empty() {
            return Err(DomainError::EmptyField { field: "nats.url" });
        }

        let subjects = NatsSubjects::new(publish_prefix, trigger_subject)?;
        Ok(Self {
            url: url_trimmed.to_owned(),
            subjects,
        })
    }
}

/// Derived subjects for every inbound and outbound channel declared
/// by the AsyncAPI spec.
#[derive(Debug, Clone)]
pub struct NatsSubjects {
    pub trigger: String,
    pub task_dispatched: String,
    pub task_completed: String,
    pub task_failed: String,
    pub deliberation_completed: String,
    pub phase_changed: String,
}

impl NatsSubjects {
    pub fn new(
        publish_prefix: impl Into<String>,
        trigger_subject: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let prefix_raw: String = publish_prefix.into();
        let trigger_raw: String = trigger_subject.into();
        let prefix = Self::validate_subject(&prefix_raw, "nats.publish_prefix")?;
        let trigger = Self::validate_subject(&trigger_raw, "nats.trigger_subject")?;
        Ok(Self {
            trigger,
            task_dispatched: format!("{prefix}.task.dispatched"),
            task_completed: format!("{prefix}.task.completed"),
            task_failed: format!("{prefix}.task.failed"),
            deliberation_completed: format!("{prefix}.deliberation.completed"),
            phase_changed: format!("{prefix}.phase.changed"),
        })
    }

    fn validate_subject(raw: &str, field: &'static str) -> Result<String, DomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField { field });
        }
        if trimmed.len() > MAX_SUBJECT_LEN {
            return Err(DomainError::FieldTooLong {
                field,
                actual: trimmed.len(),
                max: MAX_SUBJECT_LEN,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters { field });
        }
        Ok(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_derive_from_prefix() {
        let s = NatsSubjects::new("choreo", "choreo.trigger.>").unwrap();
        assert_eq!(s.trigger, "choreo.trigger.>");
        assert_eq!(s.task_dispatched, "choreo.task.dispatched");
        assert_eq!(s.task_completed, "choreo.task.completed");
        assert_eq!(s.task_failed, "choreo.task.failed");
        assert_eq!(s.deliberation_completed, "choreo.deliberation.completed");
        assert_eq!(s.phase_changed, "choreo.phase.changed");
    }

    #[test]
    fn different_prefixes_produce_different_subjects() {
        let prod = NatsSubjects::new("choreo.prod", "choreo.prod.trigger.>").unwrap();
        assert_eq!(prod.task_dispatched, "choreo.prod.task.dispatched");
    }

    #[test]
    fn empty_url_is_rejected() {
        let err = NatsConfig::new("   ", "choreo", "choreo.trigger.>").unwrap_err();
        assert!(matches!(err, DomainError::EmptyField { field: "nats.url" }));
    }

    #[test]
    fn empty_prefix_is_rejected() {
        let err = NatsSubjects::new("  ", "choreo.trigger.>").unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "nats.publish_prefix"
            }
        ));
    }

    #[test]
    fn control_characters_in_subject_are_rejected() {
        // Embedded (non-trimmable) control character must fail.
        let err = NatsSubjects::new("cho\x00reo", "choreo.trigger.>").unwrap_err();
        assert!(matches!(err, DomainError::InvalidCharacters { .. }));
    }

    #[test]
    fn config_builds_from_defaults() {
        let cfg = NatsConfig::new("nats://nats:4222", "choreo", "choreo.trigger.>").unwrap();
        assert_eq!(cfg.url, "nats://nats:4222");
        assert_eq!(cfg.subjects.trigger, "choreo.trigger.>");
    }
}
