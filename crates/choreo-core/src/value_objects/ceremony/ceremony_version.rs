use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const DEFAULT_VERSION: &str = "1.0";
const MAX_VERSION_LEN: usize = 64;

/// The version of a working session's definition.
///
/// This is the ceremony's own version, not the version of the document
/// it was written in. It was pinned to a single accepted value while
/// there was nothing to distinguish — with publication there is: a
/// published version is immutable, so a definition that changes needs
/// somewhere to change *to*.
///
/// No ordering is imposed. The engine needs to tell two versions apart,
/// not to decide which is newer; a scheme that ranked them would be
/// making a release-management decision on behalf of every consumer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CeremonyVersion(String);

impl CeremonyVersion {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyField {
                field: "ceremony_version",
            });
        }
        if trimmed.len() > MAX_VERSION_LEN {
            return Err(DomainError::FieldTooLong {
                field: "ceremony_version",
                actual: trimmed.len(),
                max: MAX_VERSION_LEN,
            });
        }
        // A version is part of a published identity, so it appears in
        // storage keys and in digests. The charset is narrow enough
        // that it never needs escaping wherever it is carried.
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_version",
            });
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The version a definition carries when its author did not choose
    /// one.
    #[must_use]
    pub fn v1() -> Self {
        Self(DEFAULT_VERSION.to_owned())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CeremonyVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_version_is_now_expressible() {
        assert_eq!(CeremonyVersion::new("2.0").unwrap().as_str(), "2.0");
        assert_eq!(CeremonyVersion::new("v2-rc1").unwrap().as_str(), "v2-rc1");
        assert_eq!(
            CeremonyVersion::new("2026-07-30").unwrap().as_str(),
            "2026-07-30"
        );
    }

    #[test]
    fn the_default_is_what_existing_documents_carry() {
        assert_eq!(CeremonyVersion::v1().as_str(), "1.0");
        assert_eq!(CeremonyVersion::new("1.0").unwrap(), CeremonyVersion::v1());
    }

    #[test]
    fn characters_that_would_need_escaping_in_a_key_are_rejected() {
        for rejected in [
            "",
            "  ",
            "1.0/2",
            "1 0",
            "1.0\u{0}",
            "a".repeat(65).as_str(),
        ] {
            assert!(
                CeremonyVersion::new(rejected).is_err(),
                "{rejected:?} was accepted"
            );
        }
    }
}
