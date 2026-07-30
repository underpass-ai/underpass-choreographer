//! Composite keys for the redb tables.
//!
//! redb has no key impl for tuples, so a key that scopes a record to a
//! ceremony and orders it within that scope is encoded by hand.
//!
//! `ceremony_id` is followed by a `0x00` separator and a big-endian
//! ordinal. Big-endian is what makes byte order match numeric order, so
//! a range scan returns a ceremony's records in the order they were
//! written. The separator is unambiguous by construction rather than by
//! convention: identifiers reject control characters, and `0x00` is
//! one, so no identifier can contain the byte that ends it.

use choreo_core::value_objects::{CeremonyId, CeremonyName, CeremonyVersion};

pub(super) const SEPARATOR: u8 = 0;
const ORDINAL_BYTES: usize = 8;

pub(super) fn scoped(ceremony_id: &CeremonyId, ordinal: u64) -> Vec<u8> {
    let id = ceremony_id.as_str().as_bytes();
    let mut key = Vec::with_capacity(id.len() + 1 + 8);
    key.extend_from_slice(id);
    key.push(SEPARATOR);
    key.extend_from_slice(&ordinal.to_be_bytes());
    key
}

/// The half-open byte range covering every record of one ceremony.
pub(super) fn scope_range(ceremony_id: &CeremonyId) -> (Vec<u8>, Vec<u8>) {
    (scoped(ceremony_id, 0), scoped(ceremony_id, u64::MAX))
}

/// The ceremony a scoped key belongs to, for scans that cross scopes.
///
/// Sliced by length, never by searching for the separator: the ordinal
/// is a big-endian `u64`, and small ordinals are mostly `0x00` bytes.
/// Looking for the last separator would split the key inside the
/// ordinal and report two records of one ceremony as belonging to two.
pub(super) fn ceremony_of(key: &[u8]) -> Option<&[u8]> {
    key.len()
        .checked_sub(ORDINAL_BYTES + 1)
        .map(|end| &key[..end])
}

/// Key for a published definition: the name length-prefixed, then the
/// version.
///
/// Length-prefixed rather than separated, because unlike a scoped
/// record this key needs no ordering — and a length prefix is
/// unambiguous without assuming anything about which characters a name
/// may contain.
pub(super) fn published(name: &CeremonyName, version: &CeremonyVersion) -> Vec<u8> {
    let name = name.as_str().as_bytes();
    let version = version.as_str().as_bytes();
    let mut key = Vec::with_capacity(2 + name.len() + version.len());
    key.extend_from_slice(&(name.len() as u16).to_be_bytes());
    key.extend_from_slice(name);
    key.extend_from_slice(version);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ceremony(raw: &str) -> CeremonyId {
        CeremonyId::new(raw).unwrap()
    }

    #[test]
    fn byte_order_matches_numeric_order() {
        let id = ceremony("c1");

        assert!(scoped(&id, 2) < scoped(&id, 10));
        assert!(scoped(&id, 10) < scoped(&id, u64::MAX));
    }

    #[test]
    fn a_scope_range_covers_only_its_own_ceremony() {
        let (start, end) = scope_range(&ceremony("c1"));
        let neighbour = scoped(&ceremony("c2"), 1);
        let prefix_neighbour = scoped(&ceremony("c11"), 1);

        assert!(scoped(&ceremony("c1"), 7) >= start);
        assert!(scoped(&ceremony("c1"), 7) <= end);
        assert!(neighbour > end);
        assert!(
            prefix_neighbour > end,
            "a ceremony whose id extends another's must not fall inside its range"
        );
    }

    #[test]
    fn a_name_and_version_boundary_cannot_be_shifted() {
        // Without the length prefix, `plan` + `1.0` and `plan1` + `.0`
        // would produce the same bytes.
        let left = published(
            &CeremonyName::new("plan").unwrap(),
            &CeremonyVersion::new("1.0").unwrap(),
        );
        let right = published(
            &CeremonyName::new("plan1").unwrap(),
            &CeremonyVersion::new(".0").unwrap(),
        );

        assert_ne!(left, right);
    }

    #[test]
    fn the_ceremony_is_recoverable_from_a_scoped_key() {
        assert_eq!(ceremony_of(&scoped(&ceremony("c1"), 3)), Some(&b"c1"[..]));
    }

    #[test]
    fn an_ordinal_full_of_separator_bytes_does_not_split_the_key() {
        // A big-endian u64 below 2^8 is seven `0x00` bytes and one
        // payload byte. Anything that located the separator by
        // searching would cut the key inside the ordinal.
        for ordinal in [0, 1, 3, 255, u64::MAX] {
            assert_eq!(
                ceremony_of(&scoped(&ceremony("ceremony-1"), ordinal)),
                Some(&b"ceremony-1"[..]),
                "ordinal {ordinal} split the key in the wrong place"
            );
        }
    }
}
