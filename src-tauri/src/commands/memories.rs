//! Memory management commands — CRUD + sharing + graph for the multiverse memory system.

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::MythicError;
use crate::models::memory::{Memory, MemoryGraph, MemoryGraphConversation, MemoryLink};
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
            "SELECT id, character_id, conversation_id, content, source, parent_id, version, is_canon, created_at
             FROM memories WHERE character_id = ? ORDER BY created_at DESC"
        )
        .bind(char_id)
        .fetch_all(&state_guard.db)
        .await?
    } else if let Some(ref conv_id) = conversation_id {
        sqlx::query_as(
            "SELECT id, character_id, conversation_id, content, source, parent_id, version, is_canon, created_at
             FROM memories WHERE conversation_id = ? ORDER BY created_at DESC"
        )
        .bind(conv_id)
        .fetch_all(&state_guard.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, character_id, conversation_id, content, source, parent_id, version, is_canon, created_at
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
    fetch_memory(&state_guard.db, &id).await
}

/// Updates a memory's content and increments its version.
#[tauri::command]
pub async fn update_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    memory_id: String,
    content: String,
) -> Result<Memory, MythicError> {
    let state_guard = state.read().await;

    let result = sqlx::query(
        "UPDATE memories SET content = ?, version = version + 1 WHERE id = ?"
    )
    .bind(&content)
    .bind(&memory_id)
    .execute(&state_guard.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Memory not found: {}", memory_id)));
    }

    info!("Updated memory: {} (version incremented)", memory_id);
    fetch_memory(&state_guard.db, &memory_id).await
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

/// Promotes a conversation-level memory to character-level canon.
/// The memory becomes visible across all conversations with this character.
#[tauri::command]
pub async fn promote_to_canon(
    state: State<'_, Arc<RwLock<AppState>>>,
    memory_id: String,
) -> Result<Memory, MythicError> {
    let state_guard = state.read().await;

    // Verify the memory exists and has a character_id
    let mem = fetch_memory(&state_guard.db, &memory_id).await?;
    if mem.character_id.is_none() {
        return Err(MythicError::Config(
            "Cannot promote a memory without a character_id to canon".to_string()
        ));
    }

    sqlx::query("UPDATE memories SET is_canon = 1 WHERE id = ?")
        .bind(&memory_id)
        .execute(&state_guard.db)
        .await?;

    info!("Promoted memory {} to canon", memory_id);
    fetch_memory(&state_guard.db, &memory_id).await
}

/// Shares a memory to another conversation by creating a link.
///
/// For 'copy' links: creates a duplicate memory in the target conversation.
/// For 'sync' links: just creates the link (no duplicate).
#[tauri::command]
pub async fn share_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    source_memory_id: String,
    target_conversation_id: String,
    link_type: Option<String>,
    direction: Option<String>,
    sync_mode: Option<String>,
) -> Result<MemoryLink, MythicError> {
    let state_guard = state.read().await;
    let link_type = link_type.unwrap_or_else(|| "copy".to_string());
    let direction = direction.unwrap_or_else(|| "one_way".to_string());
    let sync_mode = sync_mode.unwrap_or_else(|| "manual".to_string());

    // Validate
    if !matches!(link_type.as_str(), "copy" | "sync") {
        return Err(MythicError::Config("link_type must be 'copy' or 'sync'".to_string()));
    }
    if !matches!(direction.as_str(), "one_way" | "two_way") {
        return Err(MythicError::Config("direction must be 'one_way' or 'two_way'".to_string()));
    }
    if !matches!(sync_mode.as_str(), "auto" | "manual") {
        return Err(MythicError::Config("sync_mode must be 'auto' or 'manual'".to_string()));
    }

    // Fetch source memory
    let source = fetch_memory(&state_guard.db, &source_memory_id).await?;

    let link_id = Uuid::new_v4().to_string();
    let mut linked_memory_id: Option<String> = None;

    // For 'copy', create a duplicate memory in the target conversation
    if link_type == "copy" {
        let copy_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon)
             VALUES (?, ?, ?, ?, 'auto', ?, 1, 0)"
        )
        .bind(&copy_id)
        .bind(&source.character_id)
        .bind(&target_conversation_id)
        .bind(&source.content)
        .bind(&source.id)
        .execute(&state_guard.db)
        .await?;

        linked_memory_id = Some(copy_id);
    }

    // Create the link record
    sqlx::query(
        "INSERT INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&link_id)
    .bind(&source_memory_id)
    .bind(&target_conversation_id)
    .bind(&link_type)
    .bind(&direction)
    .bind(&sync_mode)
    .bind(&linked_memory_id)
    .execute(&state_guard.db)
    .await?;

    info!("Shared memory {} to conversation {} (type: {}, direction: {}, sync: {})",
        source_memory_id, target_conversation_id, link_type, direction, sync_mode);

    let row: MemoryLinkRow = sqlx::query_as(
        "SELECT id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id, created_at
         FROM memory_links WHERE id = ?"
    )
    .bind(&link_id)
    .fetch_one(&state_guard.db)
    .await?;

    Ok(row.into())
}

/// Removes a sharing link between conversations.
#[tauri::command]
pub async fn unlink_memory(
    state: State<'_, Arc<RwLock<AppState>>>,
    link_id: String,
) -> Result<(), MythicError> {
    let state_guard = state.read().await;

    let result = sqlx::query("DELETE FROM memory_links WHERE id = ?")
        .bind(&link_id)
        .execute(&state_guard.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Memory link not found: {}", link_id)));
    }

    info!("Removed memory link: {}", link_id);
    Ok(())
}

/// Returns the full memory graph for a character — all memories, links,
/// and conversations, structured for the frontend graph canvas.
#[tauri::command]
pub async fn get_memory_graph(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<MemoryGraph, MythicError> {
    let state_guard = state.read().await;

    // Get character name
    let char_name: Option<String> = sqlx::query_scalar(
        "SELECT name FROM characters WHERE id = ?"
    )
    .bind(&character_id)
    .fetch_optional(&state_guard.db)
    .await?;

    let character_name = char_name
        .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", character_id)))?;

    // All memories for this character
    let memory_rows: Vec<MemoryRow> = sqlx::query_as(
        "SELECT id, character_id, conversation_id, content, source, parent_id, version, is_canon, created_at
         FROM memories WHERE character_id = ? ORDER BY created_at ASC"
    )
    .bind(&character_id)
    .fetch_all(&state_guard.db)
    .await?;

    let memories: Vec<Memory> = memory_rows.into_iter().map(Into::into).collect();

    // All memory IDs for this character (for link query)
    let memory_ids: Vec<String> = memories.iter().map(|m| m.id.clone()).collect();

    // All links involving these memories
    let links = if memory_ids.is_empty() {
        Vec::new()
    } else {
        let placeholders = memory_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id, created_at
             FROM memory_links WHERE source_memory_id IN ({})",
            placeholders
        );
        let mut q = sqlx::query_as::<_, MemoryLinkRow>(&query);
        for id in &memory_ids {
            q = q.bind(id);
        }
        let rows: Vec<MemoryLinkRow> = q.fetch_all(&state_guard.db).await?;
        rows.into_iter().map(Into::into).collect()
    };

    // Conversations that have memories for this character
    let conv_rows: Vec<ConvSummaryRow> = sqlx::query_as(
        "SELECT c.id, c.title, COUNT(m.id) as memory_count
         FROM conversations c
         JOIN memories m ON m.conversation_id = c.id AND m.character_id = ?
         GROUP BY c.id, c.title
         ORDER BY c.updated_at DESC"
    )
    .bind(&character_id)
    .fetch_all(&state_guard.db)
    .await?;

    let conversations = conv_rows.into_iter().map(|r| MemoryGraphConversation {
        id: r.id,
        title: r.title,
        memory_count: r.memory_count,
    }).collect();

    Ok(MemoryGraph {
        character_id,
        character_name,
        memories,
        links,
        conversations,
    })
}

// --- Internal helpers ---

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    character_id: Option<String>,
    conversation_id: Option<String>,
    content: String,
    source: String,
    parent_id: Option<String>,
    version: i32,
    is_canon: bool,
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
            parent_id: row.parent_id,
            version: row.version,
            is_canon: row.is_canon,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MemoryLinkRow {
    id: String,
    source_memory_id: String,
    target_conversation_id: String,
    link_type: String,
    direction: String,
    sync_mode: String,
    linked_memory_id: Option<String>,
    created_at: String,
}

impl From<MemoryLinkRow> for MemoryLink {
    fn from(row: MemoryLinkRow) -> Self {
        MemoryLink {
            id: row.id,
            source_memory_id: row.source_memory_id,
            target_conversation_id: row.target_conversation_id,
            link_type: row.link_type,
            direction: row.direction,
            sync_mode: row.sync_mode,
            linked_memory_id: row.linked_memory_id,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ConvSummaryRow {
    id: String,
    title: String,
    memory_count: i32,
}

async fn fetch_memory(
    db: &sqlx::Pool<sqlx::Sqlite>,
    id: &str,
) -> Result<Memory, MythicError> {
    let row: MemoryRow = sqlx::query_as(
        "SELECT id, character_id, conversation_id, content, source, parent_id, version, is_canon, created_at
         FROM memories WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Memory not found: {}", id)))?;

    Ok(row.into())
}
