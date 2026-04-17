//! `OffsetDateTime` ↔ `google.protobuf.Timestamp` conversion.
//!
//! Shared helper used by any mapper that needs to put a wall-clock
//! moment on the wire. The proto `Timestamp` splits the instant into
//! `seconds: i64` + `nanos: i32`; this helper does the split from a
//! `time::OffsetDateTime` without losing precision.

use prost_types::Timestamp;
use time::OffsetDateTime;

#[must_use]
pub fn offset_to_timestamp(dt: OffsetDateTime) -> Timestamp {
    let nanos = dt.unix_timestamp_nanos();
    let seconds = i64::try_from(nanos.div_euclid(1_000_000_000)).unwrap_or(i64::MAX);
    let sub_nanos = i32::try_from(nanos.rem_euclid(1_000_000_000)).unwrap_or(0);
    Timestamp {
        seconds,
        nanos: sub_nanos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn splits_seconds_and_nanos_correctly() {
        let dt = datetime!(2026-04-15 12:00:00.500 UTC);
        let ts = offset_to_timestamp(dt);
        assert_eq!(ts.seconds, dt.unix_timestamp());
        assert_eq!(ts.nanos, 500_000_000);
    }

    #[test]
    fn epoch_roundtrips_to_zero() {
        let dt = OffsetDateTime::UNIX_EPOCH;
        let ts = offset_to_timestamp(dt);
        assert_eq!(ts.seconds, 0);
        assert_eq!(ts.nanos, 0);
    }
}
