-- Full-text search index for message content.
-- Uses SQLite FTS5 (compiled in by default) for fast substring/phrase matching.

-- Create the FTS virtual table
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    message_id UNINDEXED,
    conversation_id UNINDEXED,
    content,
    tokenize='unicode61 remove_diacritics 2'
);

-- Populate from existing messages
INSERT INTO messages_fts (message_id, conversation_id, content)
SELECT id, conversation_id, content FROM messages;

-- Keep FTS in sync: auto-index new messages
CREATE TRIGGER IF NOT EXISTS trg_messages_fts_insert
AFTER INSERT ON messages
BEGIN
    INSERT INTO messages_fts (message_id, conversation_id, content)
    VALUES (NEW.id, NEW.conversation_id, NEW.content);
END;

-- Keep FTS in sync: remove deleted messages
CREATE TRIGGER IF NOT EXISTS trg_messages_fts_delete
AFTER DELETE ON messages
BEGIN
    DELETE FROM messages_fts WHERE message_id = OLD.id;
END;

-- Keep FTS in sync: update edited messages
CREATE TRIGGER IF NOT EXISTS trg_messages_fts_update
AFTER UPDATE OF content ON messages
BEGIN
    DELETE FROM messages_fts WHERE message_id = OLD.id;
    INSERT INTO messages_fts (message_id, conversation_id, content)
    VALUES (NEW.id, NEW.conversation_id, NEW.content);
END;
