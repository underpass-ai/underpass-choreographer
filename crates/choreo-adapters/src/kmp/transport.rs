//! The pipe to a memory kernel, and what can go wrong on it.

use async_trait::async_trait;
use serde_json::Value;

/// What a kernel tool call came back with.
///
/// A tool that refuses is not a broken pipe. "There is no memory
/// under that name" is an answer, and one this adapter reads as an
/// empty scope rather than a failure; a transport that flattened the
/// two would make an unreachable kernel indistinguishable from a
/// session nobody has written about yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelAnswer {
    /// The tool ran and returned its document.
    Returned(Value),
    /// The tool ran and refused, in the kernel's own words.
    Refused(String),
}

/// Something went wrong on the way to the kernel, or on the way back.
#[derive(Debug, thiserror::Error)]
pub enum KernelTransportError {
    #[error("the memory kernel could not be started: {0}")]
    Unstartable(String),

    #[error("the memory kernel refused the opening handshake: {0}")]
    Unwelcoming(String),

    #[error("the memory kernel stopped listening")]
    Gone,

    #[error("the memory kernel did not answer within {seconds}s")]
    Silent { seconds: u64 },

    #[error("the memory kernel answered something this client cannot read: {0}")]
    Unreadable(String),
}

/// Calling one tool on a memory kernel.
///
/// One method, because that is the whole protocol: everything the
/// kernel offers is a named tool taking a JSON document. Keeping the
/// trait this narrow is what lets a test stand in for a kernel
/// without standing in for a process.
#[async_trait]
pub trait KernelTransport: Send + Sync {
    async fn call(
        &self,
        tool: &str,
        arguments: Value,
    ) -> Result<KernelAnswer, KernelTransportError>;
}
