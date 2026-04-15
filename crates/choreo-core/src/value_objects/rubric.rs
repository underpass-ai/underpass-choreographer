//! [`Rubric`] value object.
//!
//! Opaque, structured guidance given to agents and validators during
//! deliberation. The Choreographer never interprets the contents of a
//! rubric: callers attach their own domain vocabulary here, and agent
//! / validator adapters read it.
//!
//! Using an opaque wrapper (rather than `serde_json::Value` directly)
//! gives us a domain-level name, enforces non-primitive boundaries,
//! and lets us evolve the internal representation without touching
//! callers.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DomainError;

/// Soft upper bound on the number of top-level keys. Prevents accidental
/// transmission of very large, unstructured payloads through the domain.
pub const MAX_RUBRIC_KEYS: usize = 256;

/// A rubric is a map of string keys to structured JSON-shaped values.
///
/// The map is ordered ([`BTreeMap`]) to keep serialization stable and
/// simplify equality for tests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rubric(BTreeMap<String, Value>);

impl Rubric {
    pub fn new(entries: BTreeMap<String, Value>) -> Result<Self, DomainError> {
        if entries.len() > MAX_RUBRIC_KEYS {
            return Err(DomainError::OutOfRange {
                field: "rubric.keys",
                value: entries.len() as f64,
                min: 0.0,
                max: MAX_RUBRIC_KEYS as f64,
            });
        }
        for key in entries.keys() {
            if key.trim().is_empty() {
                return Err(DomainError::EmptyField {
                    field: "rubric.key",
                });
            }
        }
        Ok(Self(entries))
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    #[must_use]
    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> BTreeMap<String, Value> {
        self.0
    }
}

impl fmt::Display for Rubric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rubric({} keys)", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(key: &str, value: Value) -> BTreeMap<String, Value> {
        let mut m = BTreeMap::new();
        m.insert(key.to_owned(), value);
        m
    }

    #[test]
    fn empty_rubric_is_valid() {
        let r = Rubric::empty();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn arbitrary_keys_and_values_are_accepted() {
        let r = Rubric::new(entry("criteria", json!({"rigor": "high"}))).unwrap();
        assert_eq!(r.get("criteria"), Some(&json!({"rigor": "high"})));
    }

    #[test]
    fn blank_key_is_rejected() {
        let err = Rubric::new(entry("  ", json!(null))).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "rubric.key"
            }
        ));
    }

    #[test]
    fn too_many_keys_is_rejected() {
        let mut m = BTreeMap::new();
        for i in 0..=MAX_RUBRIC_KEYS {
            m.insert(format!("k{i}"), json!(i));
        }
        assert!(matches!(
            Rubric::new(m).unwrap_err(),
            DomainError::OutOfRange {
                field: "rubric.keys",
                ..
            }
        ));
    }

    #[test]
    fn domain_neutrality_is_preserved() {
        // The rubric layer must accept any shape from any domain.
        let cases = [
            ("software", json!({"quality": "high"})),
            ("clinical", json!({"capa_required": true})),
            ("logistics", json!({"sla_minutes": 120})),
        ];
        for (key, value) in cases {
            Rubric::new(entry(key, value)).unwrap();
        }
    }

    #[test]
    fn serde_is_transparent_map() {
        let r = Rubric::new(entry("k", json!(1))).unwrap();
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"k":1}"#);
        let back: Rubric = serde_json::from_str(&s).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn display_shows_count() {
        let r = Rubric::new(entry("k", json!(1))).unwrap();
        assert_eq!(r.to_string(), "Rubric(1 keys)");
    }
}
