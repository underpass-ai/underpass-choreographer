//! Postgres connection pool + migration runner.
//!
//! One [`PostgresPool`] per service instance; cloning yields a
//! shared-handle so it can be passed into multiple adapters (repository,
//! future registries) without re-dialing the database.

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

/// sqlx ships a migrator keyed on the workspace-relative path. The
/// `.sql` files live next to this crate's `Cargo.toml`.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

/// How the pool is configured.
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
}

impl PostgresConfig {
    #[must_use]
    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_connections: 10,
            acquire_timeout: Duration::from_secs(5),
        }
    }
}

/// Clone-friendly handle to a configured [`sqlx::PgPool`].
#[derive(Clone)]
pub struct PostgresPool {
    inner: PgPool,
}

impl std::fmt::Debug for PostgresPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresPool").finish()
    }
}

impl PostgresPool {
    /// Dial the database and return a ready pool.
    pub async fn connect(config: &PostgresConfig) -> Result<Self, PostgresPoolError> {
        let inner = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.acquire_timeout)
            .connect(&config.url)
            .await
            .map_err(PostgresPoolError::Connect)?;
        Ok(Self { inner })
    }

    /// Apply every embedded migration. Idempotent.
    pub async fn run_migrations(&self) -> Result<(), PostgresPoolError> {
        MIGRATOR
            .run(&self.inner)
            .await
            .map_err(PostgresPoolError::Migrate)
    }

    pub(crate) fn inner(&self) -> &PgPool {
        &self.inner
    }
}

#[derive(Debug, Error)]
pub enum PostgresPoolError {
    #[error("postgres connect failed: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("postgres migrations failed: {0}")]
    Migrate(#[source] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_url_sets_sensible_defaults() {
        let cfg = PostgresConfig::from_url("postgres://localhost/db");
        assert_eq!(cfg.url, "postgres://localhost/db");
        assert_eq!(cfg.max_connections, 10);
        assert_eq!(cfg.acquire_timeout, Duration::from_secs(5));
    }

    #[test]
    fn pool_error_display_is_descriptive() {
        let err = PostgresPoolError::Connect(sqlx::Error::PoolClosed);
        let shown = format!("{err}");
        assert!(shown.starts_with("postgres connect failed:"));
    }
}
