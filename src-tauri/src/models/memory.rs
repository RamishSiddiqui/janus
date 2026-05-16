use serde::{Deserialize, Serialize};

/// A persisted memory entry — user-pinned facts or AI-extracted context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub character_id: Option<String>,
    pub conversation_id: Option<String>,
    pub content: String,
    pub source: String, // "user" | "auto"
    pub created_at: String,
}
