use serde::{Deserialize, Serialize};

/// A standalone lorebook entry stored in the database.
///
/// Lorebooks are keyword-triggered world information that gets
/// injected into the prompt only when relevant keywords appear
/// in the recent chat history. This keeps the context focused
/// while allowing rich world-building.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LorebookEntry {
    pub id: String,

    /// The character this entry belongs to (None = global lorebook)
    pub character_id: Option<String>,

    /// Trigger keywords — entry activates when any keyword is found in chat.
    /// Stored as JSON array in the database.
    pub keys: Vec<String>,

    /// The content to inject into the prompt when triggered
    pub content: String,

    /// Whether this entry is active
    pub enabled: bool,

    /// If true, always injected regardless of keyword matches
    pub always_active: bool,

    /// Priority level (higher = more important when context is limited)
    pub priority: i32,

    /// Order relative to other entries in the prompt
    pub insertion_order: i32,

    /// Optional display name for the entry
    pub name: Option<String>,
}
