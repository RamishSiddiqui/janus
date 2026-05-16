use serde::{Deserialize, Serialize};

/// A generated or imported scene (image/video) tied to a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub conversation_id: String,
    pub message_id: Option<String>,
    pub media_type: String,
    pub prompt: String,
    pub file_path: String,
    pub caption: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}
