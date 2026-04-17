-- Phase 11b: council persistence.
--
-- One row per specialty (the current contract guarantees a single
-- council per specialty — `CouncilRegistryPort::get(&Specialty)`).
-- The full aggregate lives in a JSONB body; specialty stays as the
-- primary key so existence checks and deletes are trivial.

CREATE TABLE IF NOT EXISTS councils (
    specialty   TEXT PRIMARY KEY,
    council_id  TEXT NOT NULL,
    body        JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS councils_council_id_idx
    ON councils (council_id);
