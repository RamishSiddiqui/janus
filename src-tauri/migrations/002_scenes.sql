-- Scenes table: stores generated/imported media associated with conversations
-- Each scene captures a moment in the conversation with an image or video

CREATE TABLE IF NOT EXISTS scenes (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL,
    message_id TEXT,                    -- The message that triggered this scene (NULL if manual)
    media_type TEXT NOT NULL CHECK (media_type IN ('image', 'video')),
    prompt TEXT NOT NULL,               -- The prompt used for generation
    file_path TEXT NOT NULL,            -- Relative path to the media file in app data
    caption TEXT,                       -- Auto-generated or user-edited caption
    metadata TEXT,                      -- JSON: seed, steps, model, dimensions, etc.
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_scenes_conversation ON scenes(conversation_id);
CREATE INDEX IF NOT EXISTS idx_scenes_message ON scenes(message_id);
