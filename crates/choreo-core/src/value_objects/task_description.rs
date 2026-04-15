//! [`TaskDescription`] value object.
//!
//! A free-form textual prompt submitted with a task. The domain does
//! not interpret its contents (that is the job of agents and
//! validators), but it does enforce basic size bounds so an unbounded
//! payload cannot slip through the core.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Soft upper bound. Large enough to hold a rich prompt (several
/// thousand tokens) but small enough to reject obvious misuse.
pub const MAX_TASK_DESCRIPTION_LEN: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskDescription(String);

impl TaskDescription {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        if value.trim().is_empty() {
            return Err(DomainError::EmptyField {
                field: "task_description",
            });
        }
        if value.len() > MAX_TASK_DESCRIPTION_LEN {
            return Err(DomainError::FieldTooLong {
                field: "task_description",
                actual: value.len(),
                max: MAX_TASK_DESCRIPTION_LEN,
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for TaskDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for TaskDescription {
    type Error = DomainError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for TaskDescription {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_prompt_is_accepted() {
        assert_eq!(
            TaskDescription::new("Summarize the alert payload.")
                .unwrap()
                .as_str(),
            "Summarize the alert payload."
        );
    }

    #[test]
    fn whitespace_only_is_rejected() {
        assert!(matches!(
            TaskDescription::new("   \n\t").unwrap_err(),
            DomainError::EmptyField {
                field: "task_description"
            }
        ));
    }

    #[test]
    fn overlong_is_rejected() {
        let too_long = "a".repeat(MAX_TASK_DESCRIPTION_LEN + 1);
        assert!(matches!(
            TaskDescription::new(too_long).unwrap_err(),
            DomainError::FieldTooLong { .. }
        ));
    }

    #[test]
    fn multiline_is_preserved_verbatim() {
        let d = TaskDescription::new("line one\nline two").unwrap();
        assert_eq!(d.as_str(), "line one\nline two");
    }

    #[test]
    fn serde_is_transparent() {
        let d = TaskDescription::new("hi").unwrap();
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"hi\"");
    }
}
