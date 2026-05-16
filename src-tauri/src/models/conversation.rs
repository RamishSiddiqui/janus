use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The role of a message sender in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,

    /// Parent message ID — enables conversation branching.
    /// If None, this is a root message.
    pub parent_id: Option<String>,

    /// JSON metadata for attached images, generation params, etc.
    pub metadata: Option<serde_json::Value>,

    pub created_at: DateTime<Utc>,
}

/// A search result from full-text message search.
///
/// Contains the matched message plus context about the conversation
/// and character it belongs to, along with an FTS5-highlighted snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub message_id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    /// FTS5 snippet with `<mark>` tags around matched terms
    pub snippet: String,
    pub conversation_title: String,
    pub character_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A conversation session between the user and a character.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,

    /// The character associated with this conversation
    pub character_id: Option<String>,

    /// ID of the active (latest) message in the current branch
    pub active_message_id: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Parameters controlling LLM text generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}
