//! Map redb and serde failures to [`DomainError`].
//!
//! The core error enum carries static strings only — runtime detail
//! belongs in structured logs, not in a variant payload. Centralised
//! here so every redb adapter logs the original identically and
//! surfaces a small, stable set of domain variants upward.

use choreo_core::error::DomainError;

pub(super) fn store_failure(error: impl std::fmt::Display, op: &'static str) -> DomainError {
    tracing::error!(error = %error, operation = op, "redb operation failed");
    DomainError::InvariantViolated {
        reason: "redb: persistence backend failed",
    }
}

pub(super) fn encoding_failure(error: &serde_json::Error, op: &'static str) -> DomainError {
    tracing::error!(error = %error, operation = op, "redb record serde failed");
    DomainError::InvariantViolated {
        reason: "redb: stored record could not be encoded or decoded",
    }
}

/// A blocking store operation that never returned.
pub(super) fn join_failure(error: &tokio::task::JoinError, op: &'static str) -> DomainError {
    tracing::error!(error = %error, operation = op, "redb blocking task failed");
    DomainError::InvariantViolated {
        reason: "redb: storage task did not complete",
    }
}
