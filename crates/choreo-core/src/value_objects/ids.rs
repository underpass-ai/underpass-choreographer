//! Identifier value objects.
//!
//! Each domain concept that needs identity gets its own newtype so that
//! the compiler rejects mixing, e.g., an [`AgentId`] where a [`TaskId`]
//! is expected. Identifiers are opaque strings at the wire level but
//! validated for basic hygiene here.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_ID_LEN: usize = 256;

fn validate_id(field: &'static str, raw: &str) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    if trimmed.len() > MAX_ID_LEN {
        return Err(DomainError::FieldTooLong {
            field,
            actual: trimmed.len(),
            max: MAX_ID_LEN,
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(DomainError::InvalidCharacters { field });
    }
    Ok(trimmed.to_owned())
}

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident, $field:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Construct a validated identifier.
            pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
                Ok(Self(validate_id($field, &raw.into())?))
            }

            /// Borrow the underlying string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume the value object and return the raw string.
            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = DomainError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = DomainError;
            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

id_newtype!(
    /// Identifier of an [`Agent`](crate::value_objects) within the Choreographer.
    AgentId,
    "agent_id"
);

id_newtype!(
    /// Identifier of a task submitted for deliberation.
    TaskId,
    "task_id"
);

id_newtype!(
    /// Identifier of a concrete proposal produced during deliberation.
    ProposalId,
    "proposal_id"
);

id_newtype!(
    /// Identifier of a council (group of agents for a given specialty).
    CouncilId,
    "council_id"
);

id_newtype!(
    /// Identifier of a domain event as emitted by the choreographer.
    EventId,
    "event_id"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_valid_id() {
        let id = AgentId::new("agent-42").expect("should parse");
        assert_eq!(id.as_str(), "agent-42");
    }

    #[test]
    fn new_trims_whitespace() {
        let id = TaskId::new("  t-1  ").expect("should parse");
        assert_eq!(id.as_str(), "t-1");
    }

    #[test]
    fn empty_is_rejected() {
        let err = ProposalId::new("   ").expect_err("should reject");
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "proposal_id"
            }
        ));
    }

    #[test]
    fn control_chars_are_rejected() {
        let err = CouncilId::new("bad\x00id").expect_err("should reject");
        assert!(matches!(
            err,
            DomainError::InvalidCharacters {
                field: "council_id"
            }
        ));
    }

    #[test]
    fn overlong_is_rejected() {
        let too_long = "x".repeat(super::MAX_ID_LEN + 1);
        let err = EventId::new(too_long).expect_err("should reject");
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn distinct_newtypes_do_not_mix() {
        fn takes_agent(_: AgentId) {}
        let task = TaskId::new("t").unwrap();
        // The following must not compile:
        // takes_agent(task);
        let _ = task;
        takes_agent(AgentId::new("a").unwrap());
    }

    #[test]
    fn try_from_str_works() {
        let id: AgentId = "a1".try_into().unwrap();
        assert_eq!(id.as_str(), "a1");
    }

    #[test]
    fn try_from_string_works() {
        let id: TaskId = String::from("t1").try_into().unwrap();
        assert_eq!(id.as_str(), "t1");
    }

    #[test]
    fn display_matches_inner() {
        let id = AgentId::new("x").unwrap();
        assert_eq!(id.to_string(), "x");
    }

    #[test]
    fn into_inner_returns_string() {
        let id = AgentId::new("x").unwrap();
        assert_eq!(id.into_inner(), "x");
    }

    #[test]
    fn serde_roundtrip_is_transparent() {
        let id = AgentId::new("abc").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        assert_eq!(s, "\"abc\"");
        let back: AgentId = serde_json::from_str(&s).unwrap();
        assert_eq!(back, id);
    }
}
