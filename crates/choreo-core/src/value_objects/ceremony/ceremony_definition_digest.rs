use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::DomainError;

const DIGEST_BYTES: usize = 32;

/// Domain separator. Bumping it is how the digest algorithm is
/// versioned, and it keeps a digest computed under another scheme from
/// ever colliding with one computed under this.
const CANONICAL_SCHEME: &[u8] = b"underpass.choreo.ceremony-definition.v1";

/// SHA-256 identity of a published ceremony definition.
///
/// Computed over a canonical encoding of what the definition declares,
/// never over the document it arrived in: two YAML files differing in
/// whitespace, key order or comments describe the same working session
/// and must produce the same digest, while any material difference must
/// produce a different one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CeremonyDefinitionDigest([u8; DIGEST_BYTES]);

impl CeremonyDefinitionDigest {
    #[must_use]
    pub fn from_bytes(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn parse_hex(value: &str) -> Result<Self, DomainError> {
        let value = value.trim();
        if value.len() != DIGEST_BYTES * 2 {
            return Err(DomainError::InvalidCharacters {
                field: "ceremony_definition_digest",
            });
        }
        let mut bytes = [0_u8; DIGEST_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let start = index * 2;
            *byte = u8::from_str_radix(&value[start..start + 2], 16).map_err(|_| {
                DomainError::InvalidCharacters {
                    field: "ceremony_definition_digest",
                }
            })?;
        }
        Ok(Self(bytes))
    }

    /// Seal a canonical encoding into a digest.
    #[must_use]
    pub(crate) fn of_canonical_form(canonical: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(CANONICAL_SCHEME);
        hasher.update(canonical);
        Self(hasher.finalize().into())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";

        let mut hex = String::with_capacity(DIGEST_BYTES * 2);
        for byte in self.0 {
            hex.push(DIGITS[usize::from(byte >> 4)] as char);
            hex.push(DIGITS[usize::from(byte & 0x0f)] as char);
        }
        hex
    }
}

impl fmt::Display for CeremonyDefinitionDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let digest = CeremonyDefinitionDigest::from_bytes([0x5a; DIGEST_BYTES]);

        assert_eq!(
            CeremonyDefinitionDigest::parse_hex(&digest.to_hex()).unwrap(),
            digest
        );
    }

    #[test]
    fn a_malformed_digest_is_rejected() {
        assert!(CeremonyDefinitionDigest::parse_hex("beef").is_err());
        assert!(CeremonyDefinitionDigest::parse_hex(&"zz".repeat(DIGEST_BYTES)).is_err());
    }

    #[test]
    fn the_scheme_separates_this_digest_from_a_bare_hash() {
        let bare = Sha256::digest(b"payload");

        assert_ne!(
            CeremonyDefinitionDigest::of_canonical_form(b"payload").as_bytes(),
            &<[u8; DIGEST_BYTES]>::from(bare)
        );
    }
}
