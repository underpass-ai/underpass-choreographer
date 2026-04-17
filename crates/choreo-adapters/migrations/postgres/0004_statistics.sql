-- Phase 11c: statistics persistence.
--
-- Two tables so each counter update is a single atomic row write,
-- and so multi-replica deployments can increment the same counters
-- concurrently without a read-modify-write race:
--
--   statistics_totals        — one row (id = 'singleton'), holds the
--                              scalar counters shared across specialties.
--   statistics_by_specialty  — one row per specialty, deliberation count.
--
-- Adapter writes use `INSERT ... ON CONFLICT ... DO UPDATE SET
-- counter = table.counter + excluded.counter` so two replicas
-- recording concurrently both land on the row's accumulated value.

CREATE TABLE IF NOT EXISTS statistics_totals (
    id                    TEXT PRIMARY KEY,
    total_deliberations   BIGINT NOT NULL DEFAULT 0,
    total_orchestrations  BIGINT NOT NULL DEFAULT 0,
    total_duration_ms     BIGINT NOT NULL DEFAULT 0,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS statistics_by_specialty (
    specialty     TEXT PRIMARY KEY,
    deliberations BIGINT NOT NULL DEFAULT 0,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
