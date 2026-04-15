//! [`NumAgents`] value object — number of agents participating in a deliberation.

use std::fmt;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Upper bound for agents in a single deliberation. Chosen conservatively:
/// larger councils quickly hit diminishing returns on proposal diversity
/// while multiplying downstream cost.
pub const MAX_NUM_AGENTS: u32 = 64;

/// Number of agents participating in a deliberation. Must be at least
/// one — a deliberation without agents has no meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NumAgents(NonZeroU32);

impl NumAgents {
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value > MAX_NUM_AGENTS {
            return Err(DomainError::OutOfRange {
                field: "num_agents",
                value: f64::from(value),
                min: 1.0,
                max: f64::from(MAX_NUM_AGENTS),
            });
        }
        let non_zero = NonZeroU32::new(value).ok_or(DomainError::MustBeNonZero {
            field: "num_agents",
        })?;
        Ok(Self(non_zero))
    }

    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for NumAgents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.get())
    }
}

impl TryFrom<u32> for NumAgents {
    type Error = DomainError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_is_minimum() {
        assert_eq!(NumAgents::new(1).unwrap().get(), 1);
    }

    #[test]
    fn zero_is_rejected() {
        assert!(matches!(
            NumAgents::new(0).unwrap_err(),
            DomainError::MustBeNonZero {
                field: "num_agents"
            }
        ));
    }

    #[test]
    fn upper_bound_is_accepted() {
        assert!(NumAgents::new(MAX_NUM_AGENTS).is_ok());
    }

    #[test]
    fn above_upper_bound_is_rejected() {
        assert!(matches!(
            NumAgents::new(MAX_NUM_AGENTS + 1).unwrap_err(),
            DomainError::OutOfRange {
                field: "num_agents",
                ..
            }
        ));
    }

    #[test]
    fn serde_is_transparent() {
        assert_eq!(
            serde_json::to_string(&NumAgents::new(3).unwrap()).unwrap(),
            "3"
        );
    }
}
