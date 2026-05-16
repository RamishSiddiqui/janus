//! Memory management commands — CRUD for persisted character/conversation memories.

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::MythicError;
use crate::models::memory::Memory;
use crate::AppState;

/// Lists memories for a character and/or conversation.
#[tauri::command]
pub async fn list_memories(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    conversation_id: Option<String>,
) -> Result<Vec<Memory>, MythicError> {
    let state_guard = state.read().await;

    let rows: Vec<MemoryRow> = if let Some(ref char_id) = character_id {
        sqlx::query_as(
            "SELECT id, character_id, conversation_id, content, source, created_at
             FROM memories WHERE character_id = ? ORDER BY created_at DESC"
        )
        .bind(char_id)
        .fetch_all(&state_guard.db)
        .await?
    } else if let Some(ref conv_id) = conversation_id {
        sqlx::query_as(
            "SELECT id, character_id, conversation_id, content, source, created_at
             FROM memories WHERE conversation_id = ? ORDER BY created_at DESC"
        )
        .bind(conv_id)
        .fetch_all(&state_guard.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, character_id, conversation_id, content, source, created_at
             FROM memories ORDER BY created_at DESC LIMIT 100"
        )
        .fetch_all(&state_guard.db)
        .await?
    };

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Creates a new memory entry.
#[tauri::command]
pub async fn create_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: Option<String>,
    conversation_id: Option<String>,
    content: String,
    source: Option<String>,
) -> Result<Memory, MythicError> {
    let state_guard = state.read().await;
    let id = Uuid::new_v4().to_string();
    let source = source.unwrap_or_else(|| "user".to_string());

    sqlx::query(
        "INSERT INTO memories (id, character_id, conversation_id, content, source) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&character_id)
    .bind(&conversation_id)
    .bind(&content)
    .bind(&source)
    .execute(&state_guard.db)
    .await?;

    info!("Created memory: {} (source: {})", id, source);

    let row: MemoryRow = sqlx::query_as(
        "SELECT id, character_id, conversation_id, content, source, created_at FROM memories WHERE id = ?"
    )
    .bind(&id)
    .fetch_one(&state_guard.db)
    .await?;

    Ok(row.into())
}

/// Deletes a memory entry.
#[tauri::command]
pub async fn delete_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    memory_id: String,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;

    let result = sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(&memory_id)
        .execute(&state_guard.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Memory not found: {}", memory_id)));
    }

    info!("Deleted memory: {}", memory_id);
    Ok(())
}

// --- Internal helpers ---

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    character_id: Option<String>,
    conversation_id: Option<String>,
    content: String,
    source: String,
    created_at: String,
}

impl From<MemoryRow> for Memory {
    fn from(row: MemoryRow) -> Self {
        Memory {
            id: row.id,
            character_id: row.character_id,
            conversation_id: row.conversation_id,
            content: row.content,
            source: row.source,
            created_at: row.created_at,
        }
    }
}
