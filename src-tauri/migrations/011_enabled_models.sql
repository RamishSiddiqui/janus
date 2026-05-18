-- Migration 011: enabled_models
-- Tracks which models the user has explicitly enabled per provider.
-- provider_id references provider_configs.id (cascade delete).
-- model_type is 'llm' | 'image' | 'video' matching provider_type.

CREATE TABLE IF NOT EXISTS enabled_models (
    id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    provider_id  TEXT NOT NULL REFERENCES provider_configs(id) ON DELETE CASCADE,
    model_id     TEXT NOT NULL,
    model_type   TEXT NOT NULL DEFAULT 'llm',
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider_id, model_id)
);

CREATE INDEX IF NOT EXISTS idx_enabled_models_provider ON enabled_models(provider_id);
CREATE INDEX IF NOT EXISTS idx_enabled_models_enabled  ON enabled_models(enabled);
