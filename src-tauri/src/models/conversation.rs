use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// The role of a message sender in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the user
    User,
    /// Message from the AI character
    Assistant,
    /// System-level instruction (not displayed to user)
    System,
}

/// A single message within a conversation.
///
/// Messages form a tree structure via `parent_id` to support
/// conversation branching (forking from any point in history).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Message {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub conversation_id: Thing,
    pub role: MessageRole,
    pub content: String,

    /// Parent message ID — enables conversation branching.
    /// If None, this is a root message.
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub parent_id: Option<Thing>,

    /// JSON metadata for attached images, generation params, etc.
    #[specta(type = Option<crate::models::JsonValue>)]
    pub metadata: Option<serde_json::Value>,

    /// Character who sent this message (for multi-character conversations).
    /// None for user messages and single-character conversations.
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub character_id: Option<Thing>,
    /// Denormalized character name for display (avoids extra lookups).
    #[serde(default)]
    pub character_name: Option<String>,

    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,

}

/// A search result from full-text message search.
///
/// Contains the matched message plus context about the conversation
/// and character it belongs to, along with a highlighted snippet.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SearchResult {
    pub message_id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    /// Snippet with `<mark>` tags around matched terms
    pub snippet: String,
    pub conversation_title: String,
    pub character_name: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
}

/// A conversation session between the user and a character.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Conversation {
    #[serde(serialize_with = "crate::models::serialize_thing", deserialize_with = "crate::models::deserialize_thing")]
    #[specta(type = String)]
    pub id: Thing,
    pub title: String,

    /// The character associated with this conversation
    #[serde(serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub character_id: Option<Thing>,

    /// ID of the active (latest) message in the current branch
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub active_message_id: Option<Thing>,

    /// Controls how auto-extracted memories are scoped:
    /// - "character"    — shared across all conversations with this character (default)
    /// - "conversation" — isolated to this specific conversation only
    /// - "none"         — auto-save disabled for this conversation
    #[serde(default = "default_memory_scope")]
    pub memory_scope: String,

    #[serde(default)]
    pub shared_character_ids: Option<String>,

    /// If this conversation was forked from another, this points to the parent conversation.
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub parent_conversation_id: Option<Thing>,

    /// The exact message in the parent conversation where the fork happened.
    #[serde(default, serialize_with = "crate::models::serialize_option_thing", deserialize_with = "crate::models::deserialize_option_thing")]
    #[specta(type = Option<String>)]
    pub branch_point_message_id: Option<Thing>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub updated_at: String,
}

fn default_memory_scope() -> String {
    "character".to_string()
}

/// Parameters controlling LLM text generation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GenerationParams {
    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Sampling temperature (0.0 = deterministic, 2.0 = very creative)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p nucleus sampling
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Frequency penalty to reduce repetition
    #[serde(default)]
    pub frequency_penalty: f32,

    /// Presence penalty to encourage topic diversity
    #[serde(default)]
    pub presence_penalty: f32,

    /// Stop sequences that halt generation
    #[serde(default)]
    pub stop: Vec<String>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            stop: Vec::new(),
        }
    }
}

fn default_max_tokens() -> u32 {
    1024
}

fn default_temperature() -> f32 {
    0.8
}

fn default_top_p() -> f32 {
    0.95
}

/// A chat message in the OpenAI-compatible format used for API calls.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}
