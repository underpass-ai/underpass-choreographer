//! Postgres-backed adapters.
//!
//! Opt-in via the `postgres` Cargo feature. Ships a pool builder, a
//! migration runner, and concrete implementations of the persistence
//! ports. The schema (see `migrations/postgres/`) stays minimal: the
//! full domain aggregate is stored as JSONB with a few indexable
//! projections, so no wire-format-shaped table grows with provider
//! vocabulary.
//!
//! Nothing here leaks sqlx types out of the module: callers outside
//! only ever see domain ports and a `PostgresPool` handle.

mod deliberation_repository;
mod error;
mod pool;

pub use deliberation_repository::PostgresDeliberationRepository;
pub use pool::{PostgresConfig, PostgresPool, PostgresPoolError};
