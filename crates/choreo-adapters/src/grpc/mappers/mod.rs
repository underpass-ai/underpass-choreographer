//! Proto ↔ domain conversions.
//!
//! Every conversion is fallible in one direction (proto → domain can
//! fail validation) and infallible the other way (domain values are
//! already validated). Errors surface as [`DomainError`] so the RPC
//! handler can funnel them through the common
//! [`crate::grpc::domain_error_to_status`] mapping.
//!
//! [`DomainError`]: choreo_core::error::DomainError

mod agent;
mod attributes;
mod council;
mod deliberation;
mod event;
mod phase;
mod proposal;
mod task;
mod validation;

// Helpers wired by the service module.
pub(super) use attributes::attributes_from_struct;
pub(super) use council::council_summary_from;
pub(super) use deliberation::{deliberate_response_from, orchestrate_response_from};
pub(super) use event::trigger_event_from_proto;
pub(super) use task::task_from_proto;
