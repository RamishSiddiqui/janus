use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::MythicError;
use crate::models::conversation::{Conversation, Message, MessageRole};
use crate::AppState;

/// Creates a new conversation for a character.
#[tauri::command]
pub async fn create_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    title: Option<String>,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;
    let id = Uuid::new_v4().to_string();
    let title = title.unwrap_or_else(|| "New Chat".to_string());

    sqlx::query(
        "INSERT INTO conversations (id, title, character_id) VALUES (?, ?, ?)"
    )
    .bind(&id)
    .bind(&title)
    .bind(&character_id)
    .execute(&state.db)
    .await?;

    info!("Created conversation: {} ({})", title, id);
    get_conversation_by_id(&state.db, &id).await
}

/// Retrieves a single conversation by ID.
#[tauri::command]
pub async fn get_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;
    get_conversation_by_id(&state.db, &id).await
}

/// Lists conversations with pagination, ordered by most recently updated.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, Arc<RwLock<AppState>>>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<Conversation>, MythicError> {
    let state = state.read().await;
    let limit = limit.unwrap_or(50).min(200);
    let offset = offset.unwrap_or(0);

    let rows = sqlx::query_as::<_, ConversationRow>(
        "SELECT id, title, character_id, active_message_id, created_at, updated_at
         FROM conversations ORDER BY updated_at DESC LIMIT ? OFFSET ?"
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Returns the total number of conversations (for pagination).
#[tauri::command]
pub async fn count_conversations(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<u32, MythicError> {
    let state = state.read().await;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM conversations")
        .fetch_one(&state.db)
        .await?;
    Ok(count.0 as u32)
}

/// Deletes a conversation and all its messages (cascade).
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;

    let result = sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Conversation not found: {}", id)));
    }

    info!("Deleted conversation: {}", id);
    Ok(())
}

/// Retrieves all messages in a conversation, ordered chronologically.
/// Returns the linear message chain following the active branch.
#[tauri::command]
pub async fn get_conversation_messages(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<Message>, MythicError> {
    let state = state.read().await;

    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT id, conversation_id, role, content, parent_id, metadata, created_at
         FROM messages WHERE conversation_id = ? ORDER BY created_at ASC"
    )
    .bind(&conversation_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Updates the active message pointer for branch navigation.
#[tauri::command]
pub async fn set_active_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    message_id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;

    sqlx::query(
        "UPDATE conversations SET active_message_id = ?, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(&message_id)
    .bind(&conversation_id)
    .execute(&state.db)
    .await?;

    Ok(())
}

/// Updates a conversation's title.
#[tauri::command]
pub async fn update_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    title: String,
) -> Result<Conversation, MythicError> {
    let state = state.read().await;

    sqlx::query(
        "UPDATE conversations SET title = ?, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(&title)
    .bind(&id)
    .execute(&state.db)
    .await?;

    info!("Updated conversation title: {} -> {}", id, title);
    get_conversation_by_id(&state.db, &id).await
}
// --- Internal helpers ---

#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: String,
    title: String,
    character_id: Option<String>,
    active_message_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<ConversationRow> for Conversation {
    fn from(row: ConversationRow) -> Self {
        Conversation {
            id: row.id,
            title: row.title,
            character_id: row.character_id,
            active_message_id: row.active_message_id,
            created_at: parse_datetime(&row.created_at),
            updated_at: parse_datetime(&row.updated_at),
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    conversation_id: String,
    role: String,
    content: String,
    parent_id: Option<String>,
    metadata: Option<String>,
    created_at: String,
}

impl From<MessageRow> for Message {
    fn from(row: MessageRow) -> Self {
        let role = match row.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "system" => MessageRole::System,
            _ => MessageRole::User,
        };

        let metadata = row.metadata
            .and_then(|s| serde_json::from_str(&s).ok());

        Message {
            id: row.id,
            conversation_id: row.conversation_id,
            role,
            content: row.content,
            parent_id: row.parent_id,
            metadata,
            created_at: parse_datetime(&row.created_at),
        }
    }
}

fn parse_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc())
        .unwrap_or_else(|_| chrono::Utc::now())
}

pub(crate) async fn get_conversation_by_id(
    db: &sqlx::Pool<sqlx::Sqlite>,
    id: &str,
) -> Result<Conversation, MythicError> {
    let row = sqlx::query_as::<_, ConversationRow>(
        "SELECT id, title, character_id, active_message_id, created_at, updated_at
         FROM conversations WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Conversation not found: {}", id)))?;

    Ok(row.into())
}
