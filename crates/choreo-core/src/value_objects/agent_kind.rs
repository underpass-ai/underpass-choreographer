//! [`AgentKind`] value object.
//!
//! Identifier that tells the [`AgentFactoryPort`](crate::ports::AgentFactoryPort)
//! which provider adapter should materialize an agent from a
//! descriptor. The choreographer does not enumerate kinds itself —
//! operators pick the labels (`"noop"`, `"vllm"`, `"anthropic"`,
//! `"openai"`, `"rule"`, `"human"`, …) that match the factories wired
//! in their composition root.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_AGENT_KIND_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentKind(String);

impl AgentKind {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = raw.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "agent.kind",
            });
        }
        if trimmed.len() > MAX_AGENT_KIND_LEN {
            return Err(DomainError::FieldTooLong {
                field: "agent.kind",
                actual: trimmed.len(),
                max: MAX_AGENT_KIND_LEN,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters {
                field: "agent.kind",
            });
        }
        Ok(Self(trimmed))
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

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for AgentKind {
    type Error = DomainError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for AgentKind {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_label_is_accepted() {
        let k = AgentKind::new("vllm").unwrap();
        assert_eq!(k.as_str(), "vllm");
    }

    #[test]
    fn label_is_trimmed() {
        assert_eq!(AgentKind::new("  noop  ").unwrap().as_str(), "noop");
    }

    #[test]
    fn empty_is_rejected() {
        assert!(matches!(
            AgentKind::new("   ").unwrap_err(),
            DomainError::EmptyField {
                field: "agent.kind"
            }
        ));
    }

    #[test]
    fn overlong_is_rejected() {
        let err = AgentKind::new("a".repeat(MAX_AGENT_KIND_LEN + 1)).unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn control_characters_are_rejected() {
        assert!(matches!(
            AgentKind::new("bad\nkind").unwrap_err(),
            DomainError::InvalidCharacters {
                field: "agent.kind"
            }
        ));
    }

    #[test]
    fn display_matches_inner() {
        assert_eq!(AgentKind::new("rule").unwrap().to_string(), "rule");
    }

    #[test]
    fn serde_is_transparent() {
        let k = AgentKind::new("anthropic").unwrap();
        assert_eq!(serde_json::to_string(&k).unwrap(), "\"anthropic\"");
    }
}
