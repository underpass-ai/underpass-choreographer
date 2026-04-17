-- Phase 11b: agent descriptor persistence.
--
-- One row per agent id. The descriptor (id, specialty, kind,
-- attributes) is what we persist; live `AgentPort` handles are
-- rehydrated on resolve by piping the descriptor through the wired
-- `AgentFactoryPort`. That keeps the same agent set visible across
-- replicas without pickling provider-specific client state.

CREATE TABLE IF NOT EXISTS agents (
    agent_id    TEXT PRIMARY KEY,
    specialty   TEXT NOT NULL,
    kind        TEXT NOT NULL,
    attributes  JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS agents_specialty_idx
    ON agents (specialty);

CREATE INDEX IF NOT EXISTS agents_kind_idx
    ON agents (kind);
