-- Memories table: user-pinned facts or AI-extracted context anchors.
-- Each memory belongs to a character (cross-conversation) or a specific conversation.
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY NOT NULL,
    character_id TEXT,
    conversation_id TEXT,
    content TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',    -- 'user' (manually pinned) | 'auto' (AI-extracted)
    created_at DATETIME DEFAULT (datetime('now')),
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memories_character ON memories(character_id);
CREATE INDEX IF NOT EXISTS idx_memories_conversation ON memories(conversation_id);
