-- Per-conversation memory scope control.
-- Allows users to control whether auto-extracted memories are:
--   'character'    — shared across all conversations with this character (default)
--   'conversation' — isolated to this specific conversation only
--   'none'         — auto-save disabled for this conversation
ALTER TABLE conversations ADD COLUMN memory_scope TEXT NOT NULL DEFAULT 'character';
