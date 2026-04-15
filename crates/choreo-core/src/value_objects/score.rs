//! [`Score`] value object — normalized quality score in `[0.0, 1.0]`.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// A normalized quality score.
///
/// By convention a higher score is better. Scores live in the closed
/// range `[0.0, 1.0]`; NaN and infinities are rejected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Score(f64);

impl Score {
    pub const MIN: Self = Self(0.0);
    pub const MAX: Self = Self(1.0);

    pub fn new(value: f64) -> Result<Self, DomainError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(DomainError::OutOfRange {
                field: "score",
                value,
                min: 0.0,
                max: 1.0,
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.0)
    }
}

// Scores are totally ordered because we restrict the domain to
// non-NaN finite values in `[0.0, 1.0]`.
impl PartialEq for Score {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for Score {}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        // Safe: constructor rejects NaN.
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

impl TryFrom<f64> for Score {
    type Error = DomainError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_are_inclusive() {
        assert_eq!(Score::new(0.0).unwrap().get(), 0.0);
        assert_eq!(Score::new(1.0).unwrap().get(), 1.0);
    }

    #[test]
    fn mid_value_is_accepted() {
        assert!((Score::new(0.5).unwrap().get() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn nan_is_rejected() {
        assert!(matches!(
            Score::new(f64::NAN).unwrap_err(),
            DomainError::OutOfRange { field: "score", .. }
        ));
    }

    #[test]
    fn infinity_is_rejected() {
        assert!(Score::new(f64::INFINITY).is_err());
        assert!(Score::new(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn negative_is_rejected() {
        assert!(Score::new(-0.01).is_err());
    }

    #[test]
    fn above_one_is_rejected() {
        assert!(Score::new(1.01).is_err());
    }

    #[test]
    fn ordering_is_total_and_ascending() {
        let mut scores = [
            Score::new(0.9).unwrap(),
            Score::new(0.1).unwrap(),
            Score::new(0.5).unwrap(),
        ];
        scores.sort();
        assert_eq!(scores[0].get(), 0.1);
        assert_eq!(scores[1].get(), 0.5);
        assert_eq!(scores[2].get(), 0.9);
    }

    #[test]
    fn equality_is_bitwise_within_valid_domain() {
        assert_eq!(Score::new(0.25).unwrap(), Score::new(0.25).unwrap());
    }

    #[test]
    fn display_is_formatted() {
        assert_eq!(Score::new(0.5).unwrap().to_string(), "0.5000");
    }

    #[test]
    fn serde_is_transparent() {
        assert_eq!(
            serde_json::to_string(&Score::new(0.25).unwrap()).unwrap(),
            "0.25"
        );
    }
}
