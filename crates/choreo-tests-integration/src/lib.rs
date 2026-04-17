//! Integration tests for the Choreographer's adapters.
//!
//! The actual scenarios live under `tests/` and are gated by the
//! `container-tests` feature. This `lib.rs` also exposes shared
//! fixture helpers (e.g. `postgres_fixture::start`) that multiple
//! test files would otherwise duplicate.

#[cfg(feature = "container-tests")]
pub mod postgres_fixture;
