//! Typed domain errors.
//!
//! Pure domain errors only. Anything related to I/O, transport, or
//! serialization belongs to the adapter layer.

use thiserror::Error;

/// All errors that the core domain can raise.
///
/// Variants are intentionally coarse-grained at the boundary: each
/// variant names the invariant that was violated, not the primitive
/// type involved.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DomainError {
    /// A required textual field was empty or whitespace-only.
    #[error("field `{field}` must not be empty")]
    EmptyField { field: &'static str },

    /// A textual field exceeded its maximum allowed length.
    #[error("field `{field}` exceeds maximum length: {actual} > {max}")]
    FieldTooLong {
        field: &'static str,
        actual: usize,
        max: usize,
    },

    /// A textual field contained characters outside the allowed set.
    #[error("field `{field}` contains invalid characters")]
    InvalidCharacters { field: &'static str },

    /// A numeric value fell outside its allowed range.
    #[error("value `{field}` out of range: {value} not in [{min}, {max}]")]
    OutOfRange {
        field: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },

    /// A numeric value that must be non-zero was zero.
    #[error("value `{field}` must be non-zero")]
    MustBeNonZero { field: &'static str },

    /// A collection that must contain at least one element was empty.
    #[error("collection `{field}` must contain at least one element")]
    EmptyCollection { field: &'static str },

    /// A state transition was attempted from an invalid state.
    #[error("invalid state transition `{from}` -> `{to}`")]
    InvalidTransition {
        from: &'static str,
        to: &'static str,
    },

    /// An aggregate rejected a command because its preconditions were
    /// not met (e.g. registering an agent into a sealed council).
    #[error("invariant violated: {reason}")]
    InvariantViolated { reason: &'static str },

    /// A lookup in a domain registry did not resolve.
    #[error("not found: {what}")]
    NotFound { what: &'static str },

    /// A domain entity with the same identity already exists.
    #[error("already exists: {what}")]
    AlreadyExists { what: &'static str },

    /// Somebody else changed this first.
    ///
    /// A distinct variant rather than an invariant violation, because
    /// the two ask different things of a caller. An invariant that was
    /// violated will be violated again by the same call; a conflict is
    /// the expected outcome of two callers reaching the same thing at
    /// once, and the answer is to reload and decide, not to give up.
    ///
    /// Collapsing them made a caller unable to tell "try again from
    /// what is stored now" from "this can never work".
    #[error("conflict: {what} was changed by someone else first")]
    Conflict { what: &'static str },

    /// No candidate satisfied the structured output contract.
    #[error("no valid proposal satisfied output contract `{contract_id}`")]
    NoValidProposal { contract_id: String },
}
