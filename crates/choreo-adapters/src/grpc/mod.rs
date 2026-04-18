//! gRPC server adapter.
//!
//! Implements the `ChoreographerService` trait generated from the
//! `underpass.choreo.v1` proto. All RPC handlers are thin: they map
//! the incoming proto message to a domain input, delegate to a use
//! case in `choreo-app`, and map the result (or the [`DomainError`])
//! back onto a proto response or [`tonic::Status`].
//!
//! Nothing in this module adds behaviour; it is a pure transport
//! translation. Use-case or provider-specific vocabulary must never
//! leak into the server — all vocabulary belongs to the proto
//! contract.
//!
//! [`DomainError`]: choreo_core::error::DomainError

mod mappers;
mod service;
mod status;
mod stream;
pub(crate) mod tracecontext;

pub use service::{ChoreographerGrpcService, ChoreographerGrpcServiceBuilder};
pub use status::domain_error_to_status;
