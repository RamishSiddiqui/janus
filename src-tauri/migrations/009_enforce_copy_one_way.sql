-- Enforce: copy links are always one_way (a frozen snapshot is inherently directional).
-- Only sync links can be two_way (live bidirectional).

-- Fix any existing invalid data
UPDATE memory_links SET direction = 'one_way' WHERE link_type = 'copy' AND direction = 'two_way';

-- SQLite doesn't support ALTER TABLE ADD CHECK, so we recreate the table
-- with the constraint baked in.

CREATE TABLE memory_links_new (
    id TEXT PRIMARY KEY NOT NULL,
    source_memory_id TEXT NOT NULL,
    target_conversation_id TEXT NOT NULL,
    link_type TEXT NOT NULL DEFAULT 'copy' CHECK (link_type IN ('copy', 'sync')),
    direction TEXT NOT NULL DEFAULT 'one_way' CHECK (direction IN ('one_way', 'two_way')),
    sync_mode TEXT NOT NULL DEFAULT 'manual' CHECK (sync_mode IN ('auto', 'manual')),
    linked_memory_id TEXT,
    created_at DATETIME DEFAULT (datetime('now')),
    -- Copy is always one_way; only sync can be two_way
    CHECK (link_type = 'sync' OR direction = 'one_way'),
    FOREIGN KEY (source_memory_id) REFERENCES memories(id) ON DELETE CASCADE,
    FOREIGN KEY (target_conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (linked_memory_id) REFERENCES memories(id) ON DELETE SET NULL
);

INSERT INTO memory_links_new SELECT * FROM memory_links;
DROP TABLE memory_links;
ALTER TABLE memory_links_new RENAME TO memory_links;

CREATE INDEX IF NOT EXISTS idx_memory_links_source ON memory_links(source_memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_links_target ON memory_links(target_conversation_id);
