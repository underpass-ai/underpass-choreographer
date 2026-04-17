//! Map sqlx errors to [`DomainError`].
//!
//! The core domain error enum only carries static strings — runtime
//! detail belongs in structured logs, not in the variant payload.
//! This module centralises the conversion so every Postgres adapter
//! logs the original error identically and surfaces a small, stable
//! set of domain variants upward.

use choreo_core::error::DomainError;
use sqlx::Error as SqlxError;

/// Convert a generic sqlx error into a domain error, logging the
/// original through `tracing::error!` for diagnostics.
pub fn sqlx_to_domain(err: SqlxError, op: &'static str) -> DomainError {
    match err {
        SqlxError::RowNotFound => DomainError::NotFound {
            what: "deliberation",
        },
        other => {
            tracing::error!(error = %other, operation = op, "postgres operation failed");
            DomainError::InvariantViolated {
                reason: "postgres: persistence backend failed",
            }
        }
    }
}

/// Convert a JSON serialization/deserialization failure into a domain
/// error. Separate entry point so the logged reason is unambiguous.
pub fn serde_to_domain(err: &serde_json::Error, op: &'static str) -> DomainError {
    tracing::error!(error = %err, operation = op, "postgres row serde failed");
    DomainError::InvariantViolated {
        reason: "postgres: deliberation body could not be serialized",
    }
}
