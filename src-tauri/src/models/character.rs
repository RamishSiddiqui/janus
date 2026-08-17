use serde::{Deserialize, Serialize};
use specta::Type;
use surrealdb::sql::Thing;

/// Character Card V2 specification — the community standard for
/// portable AI character definitions. Compatible with SillyTavern,
/// Chub.ai, and other TavernAI-compatible platforms.
///
/// Reference: https://github.com/malfoyslastname/character-card-spec-v2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardV2 {
    /// Must be "chara_card_v2"
    pub spec: String,

    /// Must be "2.0"
    pub spec_version: String,

    /// All character data fields
    pub data: CharacterData,
}

/// The inner data payload of a Character Card V2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterData {
    /// Character's display name (required)
    pub name: String,

    /// Detailed character description — personality, appearance, backstory.
    /// Always included in the prompt.
    #[serde(default)]
    pub description: String,

    /// Brief personality summary
    #[serde(default)]
    pub personality: String,

    /// The scenario/setting context for the roleplay
    #[serde(default)]
    pub scenario: String,

    /// The character's opening message when starting a new chat
    #[serde(default)]
    pub first_mes: String,

    /// Example dialogue exchanges showing the character's voice.
    /// Blocks separated by <START> tags.
    #[serde(default)]
    pub mes_example: String,

    /// Notes from the character creator (not sent to the model)
    #[serde(default)]
    pub creator_notes: String,

    /// System-level prompt override for this character
    #[serde(default)]
    pub system_prompt: String,

    /// Instructions injected after chat history in the prompt
    #[serde(default)]
    pub post_history_instructions: String,

    /// Multiple starting messages — displayed as swipeable alternatives
    #[serde(default)]
    pub alternate_greetings: Vec<String>,

    /// Embedded lorebook for character-specific world info
    #[serde(default)]
    pub character_book: Option<CharacterBook>,

    /// Searchable tags for character discovery
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creator's name/handle
    #[serde(default)]
    pub creator: String,

    /// Version string for the character definition
    #[serde(default)]
    pub character_version: String,

    /// Flexible extension data for custom fields
    #[serde(default)]
    pub extensions: serde_json::Value,

    /// NPC-pipeline-specific: true when the story has deliberately not
    /// revealed who this character really is (a cloaked figure, a masked
    /// stranger). Harmlessly false for ordinary user-created/imported
    /// character cards, which never set it. Changes how the profile
    /// generator writes the description (stays vague, in-universe) and how
    /// portrait generation frames the prompt (concealment, not a clear face).
    #[serde(default)]
    pub identity_concealed: bool,
}

/// An embedded lorebook within a character card.
/// Contains keyword-triggered world information entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterBook {
    /// Optional name for this lorebook
    #[serde(default)]
    pub name: Option<String>,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// The lorebook entries
    #[serde(default)]
    pub entries: Vec<CharacterBookEntry>,
}

/// A single lorebook entry within a character book.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterBookEntry {
    /// Trigger keywords — entry is injected when any keyword appears in chat
    pub keys: Vec<String>,

    /// The content to inject into the prompt when triggered
    pub content: String,

    /// Whether this entry is active
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Order relative to other entries in the prompt
    #[serde(default = "default_insertion_order")]
    pub insertion_order: i32,

    /// Whether keyword matching is case-sensitive
    #[serde(default)]
    pub case_sensitive: bool,

    /// Priority level (higher = more important)
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// If true, always injected regardless of keyword matches
    #[serde(default)]
    pub constant: bool,

    /// Optional display name for the entry
    #[serde(default)]
    pub name: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_insertion_order() -> i32 {
    100
}

fn default_priority() -> i32 {
    10
}

/// Database representation of a character.
/// Stores the full V2 JSON in the `data` field as a native JSON object in SurrealDB.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Character {
    #[serde(
        serialize_with = "crate::models::serialize_thing",
        deserialize_with = "crate::models::deserialize_thing"
    )]
    #[specta(type = String)]
    pub id: Thing,
    pub name: String,
    pub spec: String,

    /// Full Character Card V2 JSON, stored as native JSON in SurrealDB
    #[specta(type = crate::models::JsonValue)]
    pub data: serde_json::Value,

    /// Path to the character's avatar image file (relative to app data dir)
    pub avatar_path: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub created_at: String,
    #[serde(default, deserialize_with = "crate::models::deserialize_datetime")]
    #[specta(type = String)]
    pub updated_at: String,

    /// "gallery" (a real standalone character) or "npc" (auto-generated by
    /// the story-driven NPC detector, scoped to its originating conversation
    /// until the user promotes it). Rows predating this field read back as
    /// "gallery" via the serde default, matching the schema's own default.
    #[serde(default = "default_character_origin")]
    pub origin: String,
    /// Only meaningful for auto-generated portraits — "pending_review" means
    /// the image exists but hasn't been approved by the user yet.
    #[serde(default = "default_portrait_status")]
    pub portrait_status: String,
    /// False only for a freshly auto-generated NPC profile the user hasn't
    /// opened yet — drives the needs-attention indicator.
    #[serde(default = "default_profile_reviewed")]
    pub profile_reviewed: bool,

    /// Set when the character is in the Trash; None means it's live.
    #[serde(
        default,
        deserialize_with = "crate::models::deserialize_option_datetime"
    )]
    #[specta(type = Option<String>)]
    pub deleted_at: Option<String>,
}

fn default_character_origin() -> String {
    "gallery".to_string()
}

fn default_portrait_status() -> String {
    "approved".to_string()
}

fn default_profile_reviewed() -> bool {
    true
}

impl CharacterCardV2 {
    /// Create a new minimal character card with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            spec: "chara_card_v2".to_string(),
            spec_version: "2.0".to_string(),
            data: CharacterData {
                name: name.into(),
                description: String::new(),
                personality: String::new(),
                scenario: String::new(),
                first_mes: String::new(),
                mes_example: String::new(),
                creator_notes: String::new(),
                system_prompt: String::new(),
                post_history_instructions: String::new(),
                alternate_greetings: Vec::new(),
                character_book: None,
                tags: Vec::new(),
                creator: String::new(),
                character_version: "1.0".to_string(),
                extensions: serde_json::Value::Object(serde_json::Map::new()),
                identity_concealed: false,
            },
        }
    }
}
