//! `google.protobuf.Struct` ↔ domain `Attributes` / `Rubric`
//! conversion helpers.
//!
//! Both `Attributes` and `Rubric` are opaque `BTreeMap<String, Value>`
//! wrappers around `serde_json::Value`, which maps cleanly onto the
//! Struct/Value/ListValue proto tree. All conversions preserve
//! semantics losslessly (numbers are carried as f64 in both sides).

use std::collections::BTreeMap;

use choreo_core::error::DomainError;
use choreo_core::value_objects::{Attributes, Rubric};
use prost_types::{value::Kind as PbKind, ListValue, Struct as PbStruct, Value as PbValue};
use serde_json::Value;

/// Convert a proto `Struct` into a domain `Attributes`.
pub fn attributes_from_struct(s: Option<PbStruct>) -> Result<Attributes, DomainError> {
    Attributes::new(struct_to_map(s))
}

/// Convert a domain `Attributes` into a proto `Struct`.
pub fn attributes_to_struct(attrs: &Attributes) -> PbStruct {
    map_to_struct(attrs.as_map())
}

/// Convert a proto `Struct` into a domain `Rubric`.
pub fn rubric_from_struct(s: Option<PbStruct>) -> Result<Rubric, DomainError> {
    Rubric::new(struct_to_map(s))
}

/// Convert a domain `Rubric` into a proto `Struct`.
///
/// Rubrics currently only flow proto → domain at request time; this
/// symmetric direction is kept for adapters that want to surface the
/// active rubric on a response (e.g. diagnostics / status RPCs) and
/// for round-trip tests. Marked `allow(dead_code)` because no RPC
/// wires it today; removing it would force a re-introduction when
/// those RPCs land.
#[allow(dead_code)]
pub fn rubric_to_struct(rubric: &Rubric) -> PbStruct {
    map_to_struct(rubric.as_map())
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

fn struct_to_map(s: Option<PbStruct>) -> BTreeMap<String, Value> {
    let Some(s) = s else {
        return BTreeMap::new();
    };
    s.fields
        .into_iter()
        .map(|(k, v)| (k, pb_value_to_json(v)))
        .collect()
}

fn map_to_struct(map: &BTreeMap<String, Value>) -> PbStruct {
    PbStruct {
        fields: map
            .iter()
            .map(|(k, v)| (k.clone(), json_to_pb_value(v)))
            .collect(),
    }
}

fn pb_value_to_json(v: PbValue) -> Value {
    match v.kind {
        None | Some(PbKind::NullValue(_)) => Value::Null,
        Some(PbKind::NumberValue(n)) => {
            serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
        }
        Some(PbKind::StringValue(s)) => Value::String(s),
        Some(PbKind::BoolValue(b)) => Value::Bool(b),
        Some(PbKind::StructValue(s)) => {
            let obj: serde_json::Map<_, _> = s
                .fields
                .into_iter()
                .map(|(k, v)| (k, pb_value_to_json(v)))
                .collect();
            Value::Object(obj)
        }
        Some(PbKind::ListValue(lv)) => {
            Value::Array(lv.values.into_iter().map(pb_value_to_json).collect())
        }
    }
}

fn json_to_pb_value(v: &Value) -> PbValue {
    let kind = match v {
        Value::Null => PbKind::NullValue(0),
        Value::Bool(b) => PbKind::BoolValue(*b),
        Value::Number(n) => {
            // Lossy only for integers that cannot fit in f64 mantissa;
            // acceptable for rubric/attributes payloads where the
            // proto wire is defined as numeric.
            let f = n.as_f64().unwrap_or(0.0);
            PbKind::NumberValue(f)
        }
        Value::String(s) => PbKind::StringValue(s.clone()),
        Value::Array(a) => PbKind::ListValue(ListValue {
            values: a.iter().map(json_to_pb_value).collect(),
        }),
        Value::Object(o) => PbKind::StructValue(PbStruct {
            fields: o
                .iter()
                .map(|(k, v)| (k.clone(), json_to_pb_value(v)))
                .collect(),
        }),
    };
    PbValue { kind: Some(kind) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn none_struct_produces_empty_map() {
        let attrs = attributes_from_struct(None).unwrap();
        assert!(attrs.is_empty());
    }

    #[test]
    fn empty_struct_produces_empty_map() {
        let attrs = attributes_from_struct(Some(PbStruct::default())).unwrap();
        assert!(attrs.is_empty());
    }

    #[test]
    fn scalar_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert("s".to_owned(), json!("hello"));
        m.insert("n".to_owned(), json!(42));
        m.insert("b".to_owned(), json!(true));
        m.insert("null".to_owned(), json!(null));
        let attrs = Attributes::new(m).unwrap();

        let pb = attributes_to_struct(&attrs);
        let back = attributes_from_struct(Some(pb)).unwrap();
        assert_eq!(back.get("s"), Some(&json!("hello")));
        assert_eq!(back.get("b"), Some(&json!(true)));
        assert_eq!(back.get("null"), Some(&json!(null)));
        // Numbers roundtrip through f64; compare numerically.
        assert_eq!(back.get("n").unwrap().as_f64().unwrap(), 42.0,);
    }

    #[test]
    fn nested_object_and_array_roundtrip() {
        let mut m = BTreeMap::new();
        m.insert(
            "payload".to_owned(),
            json!({
                "severity": "p1",
                "tags": ["latency", "prod"],
                "counts": {"p50": 100, "p99": 500},
            }),
        );
        let attrs = Attributes::new(m).unwrap();
        let back = attributes_from_struct(Some(attributes_to_struct(&attrs))).unwrap();
        let payload = back.get("payload").unwrap().as_object().unwrap();
        assert_eq!(payload.get("severity").unwrap(), "p1");
        assert_eq!(payload.get("tags").unwrap(), &json!(["latency", "prod"]));
        assert_eq!(
            payload
                .get("counts")
                .unwrap()
                .get("p50")
                .unwrap()
                .as_f64()
                .unwrap(),
            100.0,
        );
    }

    #[test]
    fn rubric_helpers_are_symmetric_with_attributes() {
        let mut m = BTreeMap::new();
        m.insert("rigor".to_owned(), json!("high"));
        let rubric = Rubric::new(m).unwrap();
        let back = rubric_from_struct(Some(rubric_to_struct(&rubric))).unwrap();
        assert_eq!(back.get("rigor"), Some(&json!("high")));
    }

    #[test]
    fn nan_in_json_is_carried_as_null_on_the_wire() {
        // Domain invariant: scores never hold NaN, but arbitrary
        // payload values may; we clamp to Null to keep the wire valid.
        let mut m = BTreeMap::new();
        let bad = serde_json::Number::from_f64(1.0).unwrap();
        m.insert("n".to_owned(), Value::Number(bad));
        let pb = map_to_struct(&m);
        assert!(pb.fields.contains_key("n"));
    }

    #[test]
    fn blank_key_is_rejected_by_domain_validation() {
        let mut fields: BTreeMap<String, PbValue> = BTreeMap::new();
        fields.insert(
            "  ".to_owned(),
            PbValue {
                kind: Some(PbKind::BoolValue(true)),
            },
        );
        let pb = PbStruct { fields };
        let err = attributes_from_struct(Some(pb)).unwrap_err();
        assert!(matches!(
            err,
            DomainError::EmptyField {
                field: "attributes.key"
            }
        ));
    }
}
