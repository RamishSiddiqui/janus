-- Character emotional state: one row per (character, conversation).
-- Updated after each AI response turn via LLM inference.
CREATE TABLE IF NOT EXISTS character_states (
    id               TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    character_id     TEXT NOT NULL,
    conversation_id  TEXT NOT NULL,
    mood             INTEGER NOT NULL DEFAULT 50,
    trust            INTEGER NOT NULL DEFAULT 50,
    arousal          INTEGER NOT NULL DEFAULT 30,
    dominant_emotion TEXT NOT NULL DEFAULT 'neutral',
    state_summary    TEXT NOT NULL DEFAULT '',
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(character_id, conversation_id),
    FOREIGN KEY (character_id)    REFERENCES characters(id)    ON DELETE CASCADE,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_char_states_char ON character_states(character_id);
CREATE INDEX IF NOT EXISTS idx_char_states_conv ON character_states(conversation_id);
