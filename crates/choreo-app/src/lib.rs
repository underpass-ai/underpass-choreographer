//! Application layer of the Underpass Choreographer.
//!
//! Use cases and application services compose the domain aggregates
//! (`choreo-core::entities`) using only the traits declared in
//! `choreo-core::ports`. No adapter types, no IO primitives, no
//! transport-shaped errors leak into this crate — the boundary is
//! enforced by the crate's dependencies.
//!
//! ## Layout
//!
//! - [`usecases`] — single-responsibility use cases. Each one is a
//!   struct holding its ports by `Arc` and exposing one `execute`.
//! - [`services`] — application services that compose multiple
//!   use cases (e.g. [`services::AutoDispatchService`]).

#![deny(missing_debug_implementations)]

pub mod services;
pub mod usecases;
