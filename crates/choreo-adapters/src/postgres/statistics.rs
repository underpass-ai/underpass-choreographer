//! Postgres implementation of [`StatisticsPort`].
//!
//! Storage: running counters split across two tables (see
//! `migrations/postgres/0004_statistics.sql`). Every record is a
//! single `INSERT ... ON CONFLICT ... DO UPDATE SET x = table.x + 1`
//! so concurrent replicas accumulate into the same row without a
//! read-modify-write race.
//!
//! Saturating semantics: Postgres `BIGINT` is `i64`. The in-memory
//! counters are `u64` so an operator who really sustained 2^63 events
//! could legally overflow on the wire. We guard by clamping at
//! read-time on snapshot; writes stay additive and never fail the
//! port.

use async_trait::async_trait;
use choreo_core::entities::Statistics;
use choreo_core::error::DomainError;
use choreo_core::ports::StatisticsPort;
use choreo_core::value_objects::{DurationMs, Specialty};
use sqlx::Row;

use super::error::sqlx_to_domain;
use super::pool::PostgresPool;

const TOTALS_SINGLETON_ID: &str = "singleton";

#[derive(Clone)]
pub struct PostgresStatistics {
    pool: PostgresPool,
}

impl std::fmt::Debug for PostgresStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresStatistics").finish()
    }
}

impl PostgresStatistics {
    #[must_use]
    pub fn new(pool: PostgresPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StatisticsPort for PostgresStatistics {
    async fn record_deliberation(
        &self,
        specialty: &Specialty,
        duration: DurationMs,
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .inner()
            .begin()
            .await
            .map_err(|e| sqlx_to_domain(e, "record_deliberation"))?;

        let duration_ms = i64::try_from(duration.get()).unwrap_or(i64::MAX);

        sqlx::query(
            "
            INSERT INTO statistics_totals
                (id, total_deliberations, total_duration_ms, updated_at)
            VALUES ($1, 1, $2, NOW())
            ON CONFLICT (id) DO UPDATE SET
                total_deliberations = statistics_totals.total_deliberations + 1,
                total_duration_ms   = statistics_totals.total_duration_ms + EXCLUDED.total_duration_ms,
                updated_at          = NOW()
            ",
        )
        .bind(TOTALS_SINGLETON_ID)
        .bind(duration_ms)
        .execute(&mut *tx)
        .await
        .map_err(|e| sqlx_to_domain(e, "record_deliberation"))?;

        sqlx::query(
            "
            INSERT INTO statistics_by_specialty (specialty, deliberations, updated_at)
            VALUES ($1, 1, NOW())
            ON CONFLICT (specialty) DO UPDATE SET
                deliberations = statistics_by_specialty.deliberations + 1,
                updated_at    = NOW()
            ",
        )
        .bind(specialty.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|e| sqlx_to_domain(e, "record_deliberation"))?;

        tx.commit()
            .await
            .map_err(|e| sqlx_to_domain(e, "record_deliberation"))?;
        Ok(())
    }

    async fn record_orchestration(&self, duration: DurationMs) -> Result<(), DomainError> {
        let duration_ms = i64::try_from(duration.get()).unwrap_or(i64::MAX);
        sqlx::query(
            "
            INSERT INTO statistics_totals
                (id, total_orchestrations, total_duration_ms, updated_at)
            VALUES ($1, 1, $2, NOW())
            ON CONFLICT (id) DO UPDATE SET
                total_orchestrations = statistics_totals.total_orchestrations + 1,
                total_duration_ms    = statistics_totals.total_duration_ms + EXCLUDED.total_duration_ms,
                updated_at           = NOW()
            ",
        )
        .bind(TOTALS_SINGLETON_ID)
        .bind(duration_ms)
        .execute(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "record_orchestration"))?;
        Ok(())
    }

    async fn snapshot(&self) -> Result<Statistics, DomainError> {
        let totals = sqlx::query(
            "
            SELECT total_deliberations, total_orchestrations, total_duration_ms
            FROM statistics_totals WHERE id = $1
            ",
        )
        .bind(TOTALS_SINGLETON_ID)
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "snapshot"))?;

        let (total_deliberations, total_orchestrations, total_duration) = match totals {
            Some(row) => {
                let td: i64 = row
                    .try_get("total_deliberations")
                    .map_err(|e| sqlx_to_domain(e, "snapshot"))?;
                let to: i64 = row
                    .try_get("total_orchestrations")
                    .map_err(|e| sqlx_to_domain(e, "snapshot"))?;
                let dur: i64 = row
                    .try_get("total_duration_ms")
                    .map_err(|e| sqlx_to_domain(e, "snapshot"))?;
                (
                    clamp_nonneg(td),
                    clamp_nonneg(to),
                    DurationMs::from_millis(clamp_nonneg(dur)),
                )
            }
            None => (0, 0, DurationMs::ZERO),
        };

        let specialty_rows =
            sqlx::query("SELECT specialty, deliberations FROM statistics_by_specialty")
                .fetch_all(self.pool.inner())
                .await
                .map_err(|e| sqlx_to_domain(e, "snapshot"))?;

        let mut per_specialty = std::collections::BTreeMap::new();
        for row in specialty_rows {
            let name: String = row
                .try_get("specialty")
                .map_err(|e| sqlx_to_domain(e, "snapshot"))?;
            let count: i64 = row
                .try_get("deliberations")
                .map_err(|e| sqlx_to_domain(e, "snapshot"))?;
            per_specialty.insert(Specialty::new(name)?, clamp_nonneg(count));
        }

        Ok(Statistics::from_counters(
            total_deliberations,
            total_orchestrations,
            total_duration,
            per_specialty,
        ))
    }
}

/// Map a signed counter onto the entity's `u64` shape. Negative
/// values are impossible with the `INSERT ... + 1` protocol but we
/// clamp defensively rather than panic on a drift.
fn clamp_nonneg(v: i64) -> u64 {
    if v < 0 {
        0
    } else {
        v as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_maps_negative_to_zero_and_preserves_positive() {
        assert_eq!(clamp_nonneg(0), 0);
        assert_eq!(clamp_nonneg(42), 42);
        assert_eq!(clamp_nonneg(-1), 0);
        assert_eq!(clamp_nonneg(i64::MAX), i64::MAX as u64);
    }
}
