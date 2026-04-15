//! [`DurationMs`] value object — durations in whole milliseconds.
//!
//! The domain uses millisecond granularity for all observable durations
//! (task runtime, deadlines, statistics) to match the gRPC/AsyncAPI
//! contracts, which are already millisecond-typed.

use std::fmt;
use std::ops::Add;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// A duration measured in whole milliseconds.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DurationMs(u64);

impl DurationMs {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_millis(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// Saturating addition so that aggregation of durations cannot
    /// overflow and silently wrap.
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl Add for DurationMs {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }
}

impl fmt::Display for DurationMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

impl From<u64> for DurationMs {
    fn from(value: u64) -> Self {
        Self::from_millis(value)
    }
}

impl TryFrom<i64> for DurationMs {
    type Error = DomainError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value < 0 {
            return Err(DomainError::OutOfRange {
                field: "duration_ms",
                value: value as f64,
                min: 0.0,
                max: f64::from(u32::MAX),
            });
        }
        Ok(Self(value as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_constant_is_zero() {
        assert_eq!(DurationMs::ZERO.get(), 0);
    }

    #[test]
    fn from_millis_is_identity() {
        assert_eq!(DurationMs::from_millis(250).get(), 250);
    }

    #[test]
    fn negative_i64_is_rejected() {
        assert!(DurationMs::try_from(-1_i64).is_err());
    }

    #[test]
    fn non_negative_i64_is_accepted() {
        assert_eq!(DurationMs::try_from(42_i64).unwrap().get(), 42);
    }

    #[test]
    fn saturating_add_cannot_overflow() {
        let big = DurationMs::from_millis(u64::MAX);
        assert_eq!(
            big.saturating_add(DurationMs::from_millis(1)).get(),
            u64::MAX
        );
    }

    #[test]
    fn add_uses_saturating_semantics() {
        let a = DurationMs::from_millis(u64::MAX - 1);
        let b = DurationMs::from_millis(10);
        assert_eq!((a + b).get(), u64::MAX);
    }

    #[test]
    fn ordering_is_natural() {
        assert!(DurationMs::from_millis(1) < DurationMs::from_millis(2));
    }

    #[test]
    fn display_includes_unit() {
        assert_eq!(DurationMs::from_millis(5).to_string(), "5ms");
    }

    #[test]
    fn serde_is_transparent() {
        assert_eq!(
            serde_json::to_string(&DurationMs::from_millis(7)).unwrap(),
            "7"
        );
    }
}
