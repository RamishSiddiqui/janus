-- Adds parent lineage tracking to branched conversations.
-- A branch conversation is a full copy of a parent conversation up to a specific message,
-- continued as a new independent chat.

ALTER TABLE conversations ADD COLUMN parent_conversation_id TEXT
  REFERENCES conversations(id) ON DELETE SET NULL;

ALTER TABLE conversations ADD COLUMN branch_point_message_id TEXT
  REFERENCES messages(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_conv_parent ON conversations(parent_conversation_id);
