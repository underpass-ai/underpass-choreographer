//! Postgres implementation of [`CouncilRegistryPort`].
//!
//! Storage shape mirrors the deliberation repository: the full
//! aggregate lives on `councils.body` as JSONB and the specialty is
//! the primary key. `register` rejects a duplicate specialty;
//! `replace` requires the council to already exist so the two
//! operations stay distinguishable from the port surface.

use async_trait::async_trait;
use choreo_core::entities::Council;
use choreo_core::error::DomainError;
use choreo_core::ports::CouncilRegistryPort;
use choreo_core::value_objects::Specialty;
use serde_json::Value as JsonValue;
use sqlx::Row;

use super::error::{serde_to_domain, sqlx_to_domain};
use super::pool::PostgresPool;

#[derive(Clone)]
pub struct PostgresCouncilRegistry {
    pool: PostgresPool,
}

impl std::fmt::Debug for PostgresCouncilRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresCouncilRegistry").finish()
    }
}

impl PostgresCouncilRegistry {
    #[must_use]
    pub fn new(pool: PostgresPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CouncilRegistryPort for PostgresCouncilRegistry {
    async fn register(&self, council: Council) -> Result<(), DomainError> {
        let body: JsonValue =
            serde_json::to_value(&council).map_err(|e| serde_to_domain(&e, "register"))?;
        let result = sqlx::query(
            "
            INSERT INTO councils (specialty, council_id, body, updated_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (specialty) DO NOTHING
            ",
        )
        .bind(council.specialty().as_str())
        .bind(council.id().as_str())
        .bind(&body)
        .execute(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "register"))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::AlreadyExists { what: "council" });
        }
        Ok(())
    }

    async fn replace(&self, council: Council) -> Result<(), DomainError> {
        let body: JsonValue =
            serde_json::to_value(&council).map_err(|e| serde_to_domain(&e, "replace"))?;
        let result = sqlx::query(
            "
            UPDATE councils SET
                council_id = $2,
                body       = $3,
                updated_at = NOW()
            WHERE specialty = $1
            ",
        )
        .bind(council.specialty().as_str())
        .bind(council.id().as_str())
        .bind(&body)
        .execute(self.pool.inner())
        .await
        .map_err(|e| sqlx_to_domain(e, "replace"))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound { what: "council" });
        }
        Ok(())
    }

    async fn get(&self, specialty: &Specialty) -> Result<Council, DomainError> {
        let row = sqlx::query("SELECT body FROM councils WHERE specialty = $1")
            .bind(specialty.as_str())
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "get"))?
            .ok_or(DomainError::NotFound { what: "council" })?;
        let body: JsonValue = row.try_get("body").map_err(|e| sqlx_to_domain(e, "get"))?;
        serde_json::from_value(body).map_err(|e| serde_to_domain(&e, "get"))
    }

    async fn list(&self) -> Result<Vec<Council>, DomainError> {
        let rows = sqlx::query("SELECT body FROM councils ORDER BY specialty")
            .fetch_all(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "list"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let body: JsonValue = row.try_get("body").map_err(|e| sqlx_to_domain(e, "list"))?;
            out.push(serde_json::from_value(body).map_err(|e| serde_to_domain(&e, "list"))?);
        }
        Ok(out)
    }

    async fn delete(&self, specialty: &Specialty) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM councils WHERE specialty = $1")
            .bind(specialty.as_str())
            .execute(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "delete"))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound { what: "council" });
        }
        Ok(())
    }

    async fn contains(&self, specialty: &Specialty) -> Result<bool, DomainError> {
        let row = sqlx::query("SELECT 1 AS one FROM councils WHERE specialty = $1")
            .bind(specialty.as_str())
            .fetch_optional(self.pool.inner())
            .await
            .map_err(|e| sqlx_to_domain(e, "contains"))?;
        Ok(row.is_some())
    }
}
