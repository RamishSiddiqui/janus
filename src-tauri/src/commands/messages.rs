use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::MythicError;
use crate::models::conversation::{Message, MessageRole};
use crate::AppState;

/// Creates a new message in a conversation.
#[tauri::command]
pub async fn create_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    role: String,
    content: String,
    parent_id: Option<String>,
    metadata: Option<serde_json::Value>,
) -> Result<Message, MythicError> {
    let state = state.read().await;
    let id = Uuid::new_v4().to_string();

    let role_str = match role.as_str() {
        "user" | "assistant" | "system" => role.as_str(),
        _ => return Err(MythicError::Validation(format!("Invalid role: {}", role))),
    };

    let metadata_str = metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content, parent_id, metadata)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&conversation_id)
    .bind(role_str)
    .bind(&content)
    .bind(&parent_id)
    .bind(&metadata_str)
    .execute(&state.db)
    .await?;

    // Update conversation's active_message_id and timestamp
    sqlx::query(
        "UPDATE conversations SET active_message_id = ?, updated_at = datetime('now')
         WHERE id = ?"
    )
    .bind(&id)
    .bind(&conversation_id)
    .execute(&state.db)
    .await?;

    info!("Created {} message in conversation {}", role_str, conversation_id);
    get_message_by_id(&state.db, &id).await
}

/// Updates a message's content (for edits).
#[tauri::command]
pub async fn update_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    content: String,
) -> Result<Message, MythicError> {
    let state = state.read().await;

    let result = sqlx::query("UPDATE messages SET content = ? WHERE id = ?")
        .bind(&content)
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Message not found: {}", id)));
    }

    info!("Updated message: {}", id);
    get_message_by_id(&state.db, &id).await
}

/// Deletes a message by ID.
#[tauri::command]
pub async fn delete_message(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;

    let result = sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Message not found: {}", id)));
    }

    info!("Deleted message: {}", id);
    Ok(())
}

/// Walks the parent_id chain to reconstruct the linear message history
/// from root to the given message. Used for building the LLM prompt.
#[tauri::command]
pub async fn get_message_branch(
    state: State<'_, Arc<RwLock<AppState>>>,
    message_id: String,
) -> Result<Vec<Message>, MythicError> {
    let state = state.read().await;

    // Collect the chain by walking parent pointers
    let mut chain = Vec::new();
    let mut current_id = Some(message_id);

    while let Some(ref id) = current_id {
        let msg = get_message_by_id(&state.db, id).await?;
        current_id = msg.parent_id.clone();
        chain.push(msg);
    }

    // Reverse so it goes root → leaf (chronological order)
    chain.reverse();
    Ok(chain)
}

/// Returns all sibling messages (messages sharing the same parent_id).
/// Used for branch navigation — shows alternates at the same conversation point.
#[tauri::command]
pub async fn get_message_siblings(
    state: State<'_, Arc<RwLock<AppState>>>,
    message_id: String,
) -> Result<Vec<Message>, MythicError> {
    let state = state.read().await;

    // First get the target message's parent_id
    let parent_id: Option<String> = sqlx::query_scalar(
        "SELECT parent_id FROM messages WHERE id = ?"
    )
    .bind(&message_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Message not found: {}", message_id)))?;

    // Find all messages with the same parent_id, ordered by creation time
    let rows = match parent_id {
        Some(ref pid) => {
            sqlx::query_as::<_, MessageRow>(
                "SELECT id, conversation_id, role, content, parent_id, metadata, created_at
                 FROM messages WHERE parent_id = ? ORDER BY created_at ASC"
            )
            .bind(pid)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            // Root messages (no parent) — get all root messages in this conversation
            let conv_id: String = sqlx::query_scalar(
                "SELECT conversation_id FROM messages WHERE id = ?"
            )
            .bind(&message_id)
            .fetch_one(&state.db)
            .await?;

            sqlx::query_as::<_, MessageRow>(
                "SELECT id, conversation_id, role, content, parent_id, metadata, created_at
                 FROM messages WHERE conversation_id = ? AND parent_id IS NULL ORDER BY created_at ASC"
            )
            .bind(&conv_id)
            .fetch_all(&state.db)
            .await?
        }
    };

    Ok(rows.into_iter().map(Into::into).collect())
}
// --- Internal helpers ---

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
            created_at: chrono::NaiveDateTime::parse_from_str(&row.created_at, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc())
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }
}

async fn get_message_by_id(
    db: &sqlx::Pool<sqlx::Sqlite>,
    id: &str,
) -> Result<Message, MythicError> {
    let row = sqlx::query_as::<_, MessageRow>(
        "SELECT id, conversation_id, role, content, parent_id, metadata, created_at
         FROM messages WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Message not found: {}", id)))?;

    Ok(row.into())
}
