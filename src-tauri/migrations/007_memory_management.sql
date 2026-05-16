-- Memory Management: adds inheritance, versioning, and cross-conversation sharing.
--
-- parent_id:  points to the canon/original memory this was forked from
-- version:    increments on each edit (for audit trail)
-- is_canon:   1 = character-level root memory (shared everywhere by default)

ALTER TABLE memories ADD COLUMN parent_id TEXT REFERENCES memories(id) ON DELETE SET NULL;
ALTER TABLE memories ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE memories ADD COLUMN is_canon INTEGER NOT NULL DEFAULT 0;

-- Junction table: tracks memory sharing between conversations.
-- Each link connects a source memory to a target conversation.
CREATE TABLE IF NOT EXISTS memory_links (
    id TEXT PRIMARY KEY NOT NULL,
    source_memory_id TEXT NOT NULL,
    target_conversation_id TEXT NOT NULL,
    -- 'copy' = one-time snapshot, 'sync' = live-linked
    link_type TEXT NOT NULL DEFAULT 'copy' CHECK (link_type IN ('copy', 'sync')),
    -- 'one_way' = source → target only, 'two_way' = bidirectional sync
    direction TEXT NOT NULL DEFAULT 'one_way' CHECK (direction IN ('one_way', 'two_way')),
    -- 'auto' = system pushes updates automatically, 'manual' = user triggers sync
    sync_mode TEXT NOT NULL DEFAULT 'manual' CHECK (sync_mode IN ('auto', 'manual')),
    -- For 'copy' links, this points to the copy created in the target conversation
    linked_memory_id TEXT,
    created_at DATETIME DEFAULT (datetime('now')),
    FOREIGN KEY (source_memory_id) REFERENCES memories(id) ON DELETE CASCADE,
    FOREIGN KEY (target_conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (linked_memory_id) REFERENCES memories(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_memory_parent ON memories(parent_id);
CREATE INDEX IF NOT EXISTS idx_memory_canon ON memories(is_canon);
CREATE INDEX IF NOT EXISTS idx_memory_links_source ON memory_links(source_memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_links_target ON memory_links(target_conversation_id);
