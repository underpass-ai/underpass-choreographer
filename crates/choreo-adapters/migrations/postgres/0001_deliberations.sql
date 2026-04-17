-- Phase 11a: deliberation persistence.
--
-- One row per deliberation, keyed by the task id that originated it.
-- The full aggregate is stored as JSONB so the schema does not couple
-- to any particular provider or use-case vocabulary. Query-friendly
-- projections (specialty, phase, winner_proposal_id) are kept as
-- plain columns for indexable filters without having to parse the
-- JSON every time.

CREATE TABLE IF NOT EXISTS deliberations (
    task_id              TEXT PRIMARY KEY,
    specialty            TEXT NOT NULL,
    phase                TEXT NOT NULL,
    winner_proposal_id   TEXT,
    body                 JSONB NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS deliberations_specialty_idx
    ON deliberations (specialty);

CREATE INDEX IF NOT EXISTS deliberations_phase_idx
    ON deliberations (phase);
