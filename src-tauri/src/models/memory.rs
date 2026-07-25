use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// A persisted memory entry — user-pinned facts or AI-extracted context.
///
/// Memories form a tree structure via `parent_id`:
/// - Canon memories (`is_canon = true`) are root nodes at the character level
/// - Conversation memories can inherit from canon via `parent_id`
/// - Sharing between conversations creates linked copies/syncs
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Memory {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub character_id: Option<Thing>,
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub conversation_id: Option<Thing>,
    pub content: String,
    pub source: String, // "user" | "auto"

    /// Parent memory this was forked/inherited from (None = root)
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub parent_id: Option<Thing>,
    /// Version counter — increments on each edit
    pub version: i32,
    /// Whether this is a character-level "canon" memory (trunk of the tree)
    pub is_canon: bool,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    pub created_at: String,

    /// Manual importance tier (1-10, default 5/neutral) used to weight
    /// retrieval ranking alongside semantic relevance and recency.
    #[serde(default = "default_importance")]
    pub importance: i32,
    /// When this memory was last surfaced to the LLM via retrieval — `None`
    /// if it has never been retrieved (including all rows predating this
    /// field, which are absent from storage rather than defaulted).
    #[serde(default, deserialize_with = "crate::models::deserialize_option_datetime")]
    pub last_accessed: Option<String>,
    /// How many times this memory has been surfaced via retrieval.
    #[serde(default)]
    pub access_count: i32,
}

pub(crate) fn default_importance() -> i32 { 5 }

/// A cross-conversation memory sharing link (SurrealDB graph edge).
///
/// Links connect a source memory to a target conversation, creating either
/// a frozen copy or a live-synced connection. Users configure:
/// - `link_type`: copy (snapshot) vs sync (live-linked)
/// - `direction`: one_way (source → target) vs two_way (bidirectional)
/// - `sync_mode`: auto (system-managed) vs manual (user-triggered)
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MemoryLink {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(alias = "in", rename(serialize = "source_memory_id", deserialize = "in"), serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(rename = "source_memory_id", type = String)]
    pub source: Thing,  // in = source memory
    #[serde(alias = "out", rename(serialize = "target_conversation_id", deserialize = "out"), serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(rename = "target_conversation_id", type = String)]
    pub target: Thing,  // out = target conversation
    pub link_type: String,       // "copy" | "sync"
    pub direction: String,       // "one_way" | "two_way"
    pub sync_mode: String,       // "auto" | "manual"
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub linked_memory_id: Option<Thing>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    pub created_at: String,
}

/// The full memory graph for a character — used by the frontend graph UI.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MemoryGraph {
    pub character_id: String,
    pub character_name: String,
    pub memories: Vec<Memory>,
    pub links: Vec<MemoryLink>,
    /// Conversations that have memories for this character
    pub conversations: Vec<MemoryGraphConversation>,
}

/// Minimal conversation info for graph rendering.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MemoryGraphConversation {
    pub id: String,
    pub title: String,
    pub character_id: String,
    pub memory_count: i32,
    /// If this conversation was branched from another, this is the parent's ID.
    pub parent_conversation_id: Option<String>,
}
