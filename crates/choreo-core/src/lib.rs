//! Domain core of the Underpass Choreographer.
//!
//! Pure domain: value objects, entities, events, and ports (traits).
//! No IO, no transport, no framework glue. Use-case agnostic and
//! provider-agnostic.
//!
//! DDD discipline applies here:
//!
//! - No primitive obsession: domain boundaries exchange value objects,
//!   never raw `String`, `u32`, `f64`, etc.
//! - Invariants are enforced at construction time (`TryFrom` / `new`)
//!   and cannot be violated by mutation.
//! - Aggregates protect their own state transitions; callers never
//!   mutate internal fields directly.
//! - Domain errors are typed; I/O errors do not exist here.

#![deny(missing_debug_implementations)]

pub mod entities;
pub mod error;
pub mod value_objects;

pub use error::DomainError;
