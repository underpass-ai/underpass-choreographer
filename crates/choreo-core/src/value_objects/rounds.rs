//! [`Rounds`] value object — number of peer-review rounds in a deliberation.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Upper bound for peer-review rounds. Chosen to keep deliberations
/// bounded and prevent accidental runaway usage of downstream agents.
pub const MAX_ROUNDS: u32 = 16;

/// Number of peer-review rounds performed during a deliberation.
///
/// Zero rounds is a valid configuration (proposals go straight to
/// validation without critique/revision). The upper bound
/// [`MAX_ROUNDS`] is enforced to keep deliberations bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rounds(u32);

impl Rounds {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value > MAX_ROUNDS {
            return Err(DomainError::OutOfRange {
                field: "rounds",
                value: f64::from(value),
                min: 0.0,
                max: f64::from(MAX_ROUNDS),
            });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl Default for Rounds {
    /// The default mirrors the Python reference implementation
    /// (`Deliberate(rounds=1)`).
    fn default() -> Self {
        Self(1)
    }
}

impl fmt::Display for Rounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<u32> for Rounds {
    type Error = DomainError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_allowed() {
        assert_eq!(Rounds::new(0).unwrap().get(), 0);
        assert_eq!(Rounds::ZERO.get(), 0);
    }

    #[test]
    fn default_is_one() {
        assert_eq!(Rounds::default().get(), 1);
    }

    #[test]
    fn upper_bound_is_max_rounds() {
        assert!(Rounds::new(MAX_ROUNDS).is_ok());
    }

    #[test]
    fn above_upper_bound_is_rejected() {
        let err = Rounds::new(MAX_ROUNDS + 1).unwrap_err();
        assert!(matches!(
            err,
            DomainError::OutOfRange {
                field: "rounds",
                ..
            }
        ));
    }

    #[test]
    fn display_is_numeric() {
        assert_eq!(Rounds::new(3).unwrap().to_string(), "3");
    }

    #[test]
    fn serde_is_transparent() {
        assert_eq!(
            serde_json::to_string(&Rounds::new(4).unwrap()).unwrap(),
            "4"
        );
    }
}
