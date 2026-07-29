use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

const HASH_BYTES: usize = 32;

/// SHA-256 digest binding one audit record to the one before it.
///
/// Kept as bytes rather than a string so the chain cannot be broken by
/// a formatting difference between implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditRecordHash([u8; HASH_BYTES]);

impl AuditRecordHash {
    #[must_use]
    pub fn from_bytes(bytes: [u8; HASH_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn parse_hex(value: &str) -> Result<Self, DomainError> {
        let value = value.trim();
        if value.len() != HASH_BYTES * 2 {
            return Err(DomainError::InvalidCharacters {
                field: "audit_record_hash",
            });
        }
        let mut bytes = [0_u8; HASH_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let start = index * 2;
            *byte = u8::from_str_radix(&value[start..start + 2], 16).map_err(|_| {
                DomainError::InvalidCharacters {
                    field: "audit_record_hash",
                }
            })?;
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; HASH_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";

        let mut hex = String::with_capacity(HASH_BYTES * 2);
        for byte in self.0 {
            hex.push(DIGITS[usize::from(byte >> 4)] as char);
            hex.push(DIGITS[usize::from(byte & 0x0f)] as char);
        }
        hex
    }
}

impl fmt::Display for AuditRecordHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let hash = AuditRecordHash::from_bytes([0xab; HASH_BYTES]);

        assert_eq!(AuditRecordHash::parse_hex(&hash.to_hex()).unwrap(), hash);
    }

    #[test]
    fn display_is_lowercase_hex_of_the_full_digest() {
        let hash = AuditRecordHash::from_bytes([0x0f; HASH_BYTES]);

        assert_eq!(hash.to_string(), "0f".repeat(HASH_BYTES));
    }

    #[test]
    fn a_truncated_or_malformed_digest_is_rejected() {
        assert!(AuditRecordHash::parse_hex("abcd").is_err());
        assert!(AuditRecordHash::parse_hex(&"zz".repeat(HASH_BYTES)).is_err());
    }
}
