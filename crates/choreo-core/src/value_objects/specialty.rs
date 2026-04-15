//! [`Specialty`] value object.
//!
//! Use-case-agnostic replacement for the SWE-specific `role` field in
//! the original service. A specialty is a free-form label chosen by
//! the operator (e.g. `"triage"`, `"investigator"`, `"reviewer"`).
//! The choreographer never enumerates specialties itself.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const MAX_SPECIALTY_LEN: usize = 128;

/// A specialty label identifying a kind of agent expertise.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Specialty(String);

impl Specialty {
    /// Construct a specialty after validating it.
    ///
    /// Accepts any non-empty, non-whitespace-only label up to
    /// `MAX_SPECIALTY_LEN` chars, free of control characters. The
    /// label is trimmed.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = raw.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField { field: "specialty" });
        }
        if trimmed.len() > MAX_SPECIALTY_LEN {
            return Err(DomainError::FieldTooLong {
                field: "specialty",
                actual: trimmed.len(),
                max: MAX_SPECIALTY_LEN,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::InvalidCharacters { field: "specialty" });
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

impl fmt::Display for Specialty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for Specialty {
    type Error = DomainError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for Specialty {
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
        let s = Specialty::new("triage").unwrap();
        assert_eq!(s.as_str(), "triage");
    }

    #[test]
    fn label_is_trimmed() {
        assert_eq!(Specialty::new("  planner  ").unwrap().as_str(), "planner");
    }

    #[test]
    fn empty_is_rejected() {
        let err = Specialty::new("   ").unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField { field: "specialty" }
        ));
    }

    #[test]
    fn control_characters_are_rejected() {
        assert!(matches!(
            Specialty::new("bad\nrole").unwrap_err(),
            DomainError::InvalidCharacters { field: "specialty" }
        ));
    }

    #[test]
    fn overlong_is_rejected() {
        let err = Specialty::new("a".repeat(MAX_SPECIALTY_LEN + 1)).unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn display_is_inner() {
        assert_eq!(Specialty::new("x").unwrap().to_string(), "x");
    }

    #[test]
    fn no_enum_of_known_specialties_exists() {
        // Regression test: the Choreographer must accept arbitrary
        // operator-defined specialties, not restrict to a fixed set.
        for label in [
            "triage",
            "investigator",
            "reviewer",
            "quality-check",
            "anomaly-scout",
            "clinical-intake",
            "supply-sourcing",
        ] {
            Specialty::new(label).unwrap_or_else(|e| panic!("{label}: {e}"));
        }
    }

    #[test]
    fn serde_is_transparent() {
        let s = Specialty::new("x").unwrap();
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"x\"");
    }
}
