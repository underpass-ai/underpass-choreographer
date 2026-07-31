//! Map kernel failures to [`DomainError`].
//!
//! The core error enum carries static strings only — runtime detail
//! belongs in structured logs, not in a variant payload. Centralised
//! here so every call logs the original identically and surfaces a
//! small, stable set of domain variants upward.

use choreo_core::error::DomainError;

use super::transport::KernelTransportError;

pub(super) fn unreachable_kernel(error: &KernelTransportError, tool: &'static str) -> DomainError {
    tracing::error!(error = %error, tool, "the memory kernel could not be reached");
    DomainError::InvariantViolated {
        reason: "kmp: the memory kernel could not be reached",
    }
}

/// A refusal the caller above did not know how to read.
///
/// Deliberately not folded into emptiness: a kernel that refuses for
/// a reason this adapter does not recognise has not told us the scope
/// is empty, and answering "nothing is remembered" would be inventing
/// the one answer that is indistinguishable from working.
pub(super) fn refused(refusal: &str, tool: &'static str) -> DomainError {
    tracing::error!(refusal, tool, "the memory kernel refused");
    DomainError::InvariantViolated {
        reason: "kmp: the memory kernel refused the call",
    }
}
