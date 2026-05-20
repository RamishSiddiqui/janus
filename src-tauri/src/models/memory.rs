use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

/// A persisted memory entry — user-pinned facts or AI-extracted context.
///
/// Memories form a tree structure via `parent_id`:
/// - Canon memories (`is_canon = true`) are root nodes at the character level
/// - Conversation memories can inherit from canon via `parent_id`
/// - Sharing between conversations creates linked copies/syncs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    pub id: Thing,
    #[serde(serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    pub character_id: Option<Thing>,
    #[serde(serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    pub conversation_id: Option<Thing>,
    pub content: String,
    pub source: String, // "user" | "auto"

    /// Parent memory this was forked/inherited from (None = root)
    #[serde(serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    pub parent_id: Option<Thing>,
    /// Version counter — increments on each edit
    pub version: i32,
    /// Whether this is a character-level "canon" memory (trunk of the tree)
    pub is_canon: bool,

    pub created_at: String,
}

/// A cross-conversation memory sharing link (SurrealDB graph edge).
///
/// Links connect a source memory to a target conversation, creating either
/// a frozen copy or a live-synced connection. Users configure:
/// - `link_type`: copy (snapshot) vs sync (live-linked)
/// - `direction`: one_way (source → target) vs two_way (bidirectional)
/// - `sync_mode`: auto (system-managed) vs manual (user-triggered)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLink {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    pub id: Thing,
    #[serde(rename = "in", serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    pub source: Thing,  // in = source memory
    #[serde(rename = "out", serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    pub target: Thing,  // out = target conversation
    pub link_type: String,       // "copy" | "sync"
    pub direction: String,       // "one_way" | "two_way"
    pub sync_mode: String,       // "auto" | "manual"
    #[serde(serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    pub linked_memory_id: Option<Thing>,
    pub created_at: String,
}

/// The full memory graph for a character — used by the frontend graph UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGraph {
    pub character_id: String,
    pub character_name: String,
    pub memories: Vec<Memory>,
    pub links: Vec<MemoryLink>,
    /// Conversations that have memories for this character
    pub conversations: Vec<MemoryGraphConversation>,
}

/// Minimal conversation info for graph rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGraphConversation {
    pub id: String,
    pub title: String,
    pub character_id: String,
    pub memory_count: i32,
    /// If this conversation was branched from another, this is the parent's ID.
    pub parent_conversation_id: Option<String>,
}
