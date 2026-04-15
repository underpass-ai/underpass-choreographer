//! [`Attributes`] value object.
//!
//! Opaque bag of structured data attached to a task, proposal, or event.
//! Use-case-specific payloads (alert envelopes, case records, document
//! references, …) ride here without leaking domain vocabulary into the
//! choreographer core.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DomainError;

/// Soft upper bound on the number of top-level attribute keys.
pub const MAX_ATTRIBUTES_KEYS: usize = 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Attributes(BTreeMap<String, Value>);

impl Attributes {
    pub fn new(entries: BTreeMap<String, Value>) -> Result<Self, DomainError> {
        if entries.len() > MAX_ATTRIBUTES_KEYS {
            return Err(DomainError::OutOfRange {
                field: "attributes.keys",
                value: entries.len() as f64,
                min: 0.0,
                max: MAX_ATTRIBUTES_KEYS as f64,
            });
        }
        for key in entries.keys() {
            if key.trim().is_empty() {
                return Err(DomainError::EmptyField {
                    field: "attributes.key",
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

impl fmt::Display for Attributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Attributes({} keys)", self.0.len())
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
    fn empty_is_allowed() {
        assert!(Attributes::empty().is_empty());
    }

    #[test]
    fn blank_key_is_rejected() {
        let err = Attributes::new(entry("  ", json!(true))).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "attributes.key"
            }
        ));
    }

    #[test]
    fn too_many_keys_is_rejected() {
        let mut m = BTreeMap::new();
        for i in 0..=MAX_ATTRIBUTES_KEYS {
            m.insert(format!("k{i}"), json!(i));
        }
        assert!(matches!(
            Attributes::new(m).unwrap_err(),
            DomainError::OutOfRange {
                field: "attributes.keys",
                ..
            }
        ));
    }

    #[test]
    fn arbitrary_payload_is_accepted() {
        let attrs = Attributes::new(entry(
            "alert",
            json!({"source": "grafana", "severity": "p1"}),
        ))
        .unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs.get("alert").unwrap()["severity"], json!("p1"));
    }

    #[test]
    fn serde_is_transparent() {
        let a = Attributes::new(entry("k", json!("v"))).unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), r#"{"k":"v"}"#);
    }
}
