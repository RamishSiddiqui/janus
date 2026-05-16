-- Initial database schema for Mythic
-- Creates tables for characters, conversations, messages, lorebook entries, and provider configs.

-- Characters table: stores Character Card V2 data
CREATE TABLE IF NOT EXISTS characters (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    spec TEXT NOT NULL DEFAULT 'chara_card_v2',
    data TEXT NOT NULL,  -- Full Character Card V2 JSON
    avatar_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Conversations table: chat sessions linked to characters
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL DEFAULT 'New Chat',
    character_id TEXT,
    active_message_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE SET NULL
);

-- Messages table: individual messages within conversations
-- Uses parent_id for tree-structured branching conversations
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    parent_id TEXT,
    metadata TEXT,  -- JSON for images, generation params, etc.
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES messages(id) ON DELETE SET NULL
);

-- Lorebook entries table: keyword-triggered world information
CREATE TABLE IF NOT EXISTS lorebook_entries (
    id TEXT PRIMARY KEY NOT NULL,
    character_id TEXT,  -- NULL = global lorebook entry
    keys TEXT NOT NULL,  -- JSON array of trigger keywords
    content TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    always_active INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 10,
    insertion_order INTEGER NOT NULL DEFAULT 100,
    name TEXT,
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
);

-- Provider configs table: AI provider connection settings
CREATE TABLE IF NOT EXISTS provider_configs (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL CHECK (provider_type IN ('llm', 'image', 'video')),
    adapter TEXT NOT NULL,
    config TEXT NOT NULL,  -- JSON adapter-specific configuration
    is_default INTEGER NOT NULL DEFAULT 0
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_parent ON messages(parent_id);
CREATE INDEX IF NOT EXISTS idx_conversations_character ON conversations(character_id);
CREATE INDEX IF NOT EXISTS idx_lorebook_character ON lorebook_entries(character_id);
CREATE INDEX IF NOT EXISTS idx_provider_type ON provider_configs(provider_type);
