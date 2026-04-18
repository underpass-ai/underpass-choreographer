//! [`TraceContext`] value object — W3C Trace Context `traceparent` header.
//!
//! Spec: <https://www.w3.org/TR/trace-context/#traceparent-header>
//!
//! Shape: `VERSION-TRACE_ID-PARENT_ID-TRACE_FLAGS`
//! - `VERSION`      — two hex digits, `"00"` today.
//! - `TRACE_ID`     — 32 lowercase hex digits, non-zero.
//! - `PARENT_ID`    — 16 lowercase hex digits (span id), non-zero.
//! - `TRACE_FLAGS`  — two hex digits.
//!
//! The choreographer does not run an OpenTelemetry SDK itself (yet);
//! this value object is just the wire-shape helper. Adapters stamp
//! it on NATS headers so external OTel-aware consumers can correlate.
//!
//! Randomly-generated ids are the honest default when no upstream
//! context is present: `TraceContext::generate()` fills each field
//! with `uuid::Uuid::new_v4()` bytes projected onto the required
//! width.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

/// Fixed width in hex characters per W3C spec.
const TRACE_ID_HEX_LEN: usize = 32;
const SPAN_ID_HEX_LEN: usize = 16;
const FLAGS_HEX_LEN: usize = 2;
const VERSION_HEX_LEN: usize = 2;

/// The supported version byte. Upstream clients sending a newer
/// version are tolerated — the extra fields are ignored — but we
/// only ever format `"00"` on our side.
const VERSION: &str = "00";

/// W3C `traceparent` shape. All components are lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    trace_id: String,
    span_id: String,
    flags: String,
}

impl TraceContext {
    /// Parse a `traceparent` header value. Accepts any version but
    /// only reads the first four dash-separated fields (future
    /// versions append; we ignore anything past the flags).
    pub fn parse(header: &str) -> Result<Self, DomainError> {
        // Spec-mandated total length is ≥ 55 for version 00 (and
        // exactly 55 for the current version). Reject obvious
        // truncation early.
        let parts: Vec<&str> = header.splitn(5, '-').collect();
        if parts.len() < 4 {
            return Err(DomainError::InvalidCharacters {
                field: "traceparent",
            });
        }
        validate_hex(parts[0], VERSION_HEX_LEN, "traceparent.version")?;
        validate_hex_nonzero(parts[1], TRACE_ID_HEX_LEN, "traceparent.trace_id")?;
        validate_hex_nonzero(parts[2], SPAN_ID_HEX_LEN, "traceparent.span_id")?;
        validate_hex(parts[3], FLAGS_HEX_LEN, "traceparent.flags")?;
        Ok(Self {
            trace_id: parts[1].to_ascii_lowercase(),
            span_id: parts[2].to_ascii_lowercase(),
            flags: parts[3].to_ascii_lowercase(),
        })
    }

    /// Build a trace context from caller-supplied hex strings. The
    /// callers using this are typically crossing a boundary where
    /// the wire format is already validated; still, we re-check to
    /// keep the value-object invariant watertight.
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        flags: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let trace_id = trace_id.into().to_ascii_lowercase();
        let span_id = span_id.into().to_ascii_lowercase();
        let flags = flags.into().to_ascii_lowercase();
        validate_hex_nonzero(&trace_id, TRACE_ID_HEX_LEN, "traceparent.trace_id")?;
        validate_hex_nonzero(&span_id, SPAN_ID_HEX_LEN, "traceparent.span_id")?;
        validate_hex(&flags, FLAGS_HEX_LEN, "traceparent.flags")?;
        Ok(Self {
            trace_id,
            span_id,
            flags,
        })
    }

    /// Generate a fresh context with random ids and flags = `"01"`
    /// (sampled). Used when the choreographer originates a trace
    /// (no upstream traceparent available).
    #[must_use]
    pub fn generate() -> Self {
        let trace_uuid = Uuid::new_v4();
        let span_bytes = Uuid::new_v4().into_bytes();
        let trace_id = hex_encode(trace_uuid.as_bytes());
        // Span id is 8 bytes (16 hex); take the leading 8 of a v4 uuid.
        let mut span_id = hex_encode(&span_bytes[..8]);
        // Guarantee non-zero — vanishingly unlikely but the W3C
        // spec explicitly forbids all-zero ids.
        if span_id.bytes().all(|c| c == b'0') {
            span_id.replace_range(0..1, "1");
        }
        Self {
            trace_id,
            span_id,
            flags: "01".to_owned(),
        }
    }

    #[must_use]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    #[must_use]
    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    #[must_use]
    pub fn flags(&self) -> &str {
        &self.flags
    }

    /// Serialize back to the `traceparent` header format.
    #[must_use]
    pub fn to_header(&self) -> String {
        format!(
            "{VERSION}-{}-{}-{}",
            self.trace_id, self.span_id, self.flags
        )
    }
}

impl fmt::Display for TraceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_header())
    }
}

fn validate_hex(s: &str, expected_len: usize, field: &'static str) -> Result<(), DomainError> {
    if s.len() != expected_len {
        return Err(DomainError::FieldTooLong {
            field,
            actual: s.len(),
            max: expected_len,
        });
    }
    if !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(DomainError::InvalidCharacters { field });
    }
    Ok(())
}

fn validate_hex_nonzero(
    s: &str,
    expected_len: usize,
    field: &'static str,
) -> Result<(), DomainError> {
    validate_hex(s, expected_len, field)?;
    if s.bytes().all(|c| c == b'0') {
        return Err(DomainError::InvariantViolated {
            reason: "traceparent: trace_id and span_id must not be all zeros",
        });
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // write! into a String is infallible.
        write!(out, "{byte:02x}").unwrap();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

    #[test]
    fn parse_accepts_the_w3c_example() {
        let ctx = TraceContext::parse(SAMPLE).unwrap();
        assert_eq!(ctx.trace_id(), "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.span_id(), "b7ad6b7169203331");
        assert_eq!(ctx.flags(), "01");
    }

    #[test]
    fn to_header_roundtrips() {
        let ctx = TraceContext::parse(SAMPLE).unwrap();
        assert_eq!(ctx.to_header(), SAMPLE);
    }

    #[test]
    fn parse_is_case_insensitive_but_stores_lowercase() {
        let upper = "00-0AF7651916CD43DD8448EB211C80319C-B7AD6B7169203331-01";
        let ctx = TraceContext::parse(upper).unwrap();
        assert_eq!(ctx.trace_id(), "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.span_id(), "b7ad6b7169203331");
    }

    #[test]
    fn parse_tolerates_trailing_fields_from_future_versions() {
        // Spec says unknown trailing fields are ignored on an
        // unknown version; we apply the same leniency on version
        // 00 too to keep behaviour uniform.
        let extended = format!("{SAMPLE}-extra-future-stuff");
        let ctx = TraceContext::parse(&extended).unwrap();
        assert_eq!(ctx.trace_id(), "0af7651916cd43dd8448eb211c80319c");
    }

    #[test]
    fn parse_rejects_wrong_trace_id_length() {
        let bad = "00-0af7651916cd43dd8448eb211c80319-b7ad6b7169203331-01"; // 31 hex
        let err = TraceContext::parse(bad).unwrap_err();
        assert!(matches!(err, DomainError::FieldTooLong { .. }));
    }

    #[test]
    fn parse_rejects_all_zero_trace_id() {
        let bad = "00-00000000000000000000000000000000-b7ad6b7169203331-01";
        let err = TraceContext::parse(bad).unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn parse_rejects_all_zero_span_id() {
        let bad = "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01";
        let err = TraceContext::parse(bad).unwrap_err();
        assert!(matches!(err, DomainError::InvariantViolated { .. }));
    }

    #[test]
    fn parse_rejects_non_hex_characters() {
        let bad = "00-gggggggggggggggggggggggggggggggg-b7ad6b7169203331-01";
        let err = TraceContext::parse(bad).unwrap_err();
        assert!(matches!(err, DomainError::InvalidCharacters { .. }));
    }

    #[test]
    fn parse_rejects_truncated_header() {
        let err = TraceContext::parse("00-0af76").unwrap_err();
        assert!(matches!(err, DomainError::InvalidCharacters { .. }));
    }

    #[test]
    fn generate_produces_valid_parseable_context() {
        let ctx = TraceContext::generate();
        let parsed = TraceContext::parse(&ctx.to_header()).unwrap();
        assert_eq!(parsed, ctx);
        assert_eq!(ctx.flags(), "01");
        assert_eq!(ctx.trace_id().len(), TRACE_ID_HEX_LEN);
        assert_eq!(ctx.span_id().len(), SPAN_ID_HEX_LEN);
    }

    #[test]
    fn generate_produces_distinct_ids_on_successive_calls() {
        let a = TraceContext::generate();
        let b = TraceContext::generate();
        assert_ne!(a.trace_id(), b.trace_id());
    }

    #[test]
    fn display_matches_to_header() {
        let ctx = TraceContext::parse(SAMPLE).unwrap();
        assert_eq!(format!("{ctx}"), SAMPLE);
    }

    #[test]
    fn serde_roundtrip_preserves_fields() {
        let ctx = TraceContext::parse(SAMPLE).unwrap();
        let json = serde_json::to_string(&ctx).unwrap();
        let back: TraceContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx, back);
    }
}
