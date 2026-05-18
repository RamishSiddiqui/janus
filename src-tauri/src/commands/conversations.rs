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
        "SELECT c.id, c.title, c.character_id, c.active_message_id, c.memory_scope, c.created_at, c.updated_at,
         c.parent_conversation_id, c.branch_point_message_id,
         (SELECT GROUP_CONCAT(DISTINCT m.character_id) FROM memories m WHERE m.conversation_id = c.id AND m.character_id != c.character_id AND m.character_id IS NOT NULL) as shared_character_ids
         FROM conversations c ORDER BY c.updated_at DESC LIMIT ? OFFSET ?"
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

/// Updates the memory scope for a conversation.
#[tauri::command]
pub async fn set_memory_scope(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    scope: String,
) -> Result<(), MythicError> {
    // Validate scope value
    if !matches!(scope.as_str(), "character" | "conversation" | "none") {
        return Err(MythicError::Config(format!(
            "Invalid memory scope '{}'. Must be 'character', 'conversation', or 'none'", scope
        )));
    }

    let state = state.read().await;
    sqlx::query("UPDATE conversations SET memory_scope = ? WHERE id = ?")
        .bind(&scope)
        .bind(&conversation_id)
        .execute(&state.db)
        .await?;

    info!("Set memory scope for conversation {} to '{}'", conversation_id, scope);
    Ok(())
}

// --- Internal helpers ---

#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: String,
    title: String,
    character_id: Option<String>,
    active_message_id: Option<String>,
    memory_scope: String,
    created_at: String,
    updated_at: String,
    shared_character_ids: Option<String>,
    parent_conversation_id: Option<String>,
    branch_point_message_id: Option<String>,
}

impl From<ConversationRow> for Conversation {
    fn from(row: ConversationRow) -> Self {
        Conversation {
            id: row.id,
            title: row.title,
            character_id: row.character_id,
            active_message_id: row.active_message_id,
            memory_scope: row.memory_scope,
            shared_character_ids: row.shared_character_ids,
            parent_conversation_id: row.parent_conversation_id,
            branch_point_message_id: row.branch_point_message_id,
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
        "SELECT c.id, c.title, c.character_id, c.active_message_id, c.memory_scope, c.created_at, c.updated_at,
         c.parent_conversation_id, c.branch_point_message_id,
         (SELECT GROUP_CONCAT(DISTINCT m.character_id) FROM memories m WHERE m.conversation_id = c.id AND m.character_id != c.character_id AND m.character_id IS NOT NULL) as shared_character_ids
         FROM conversations c WHERE c.id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Conversation not found: {}", id)))?;

    Ok(row.into())
}

// --- Branch Conversation ---

/// Creates a new conversation that is a branch of an existing one.
///
/// The new conversation contains a full copy of all messages up to and including
/// `branch_point_message_id`, preserving the parent→child chain with fresh IDs.
///
/// All memories from the parent conversation are bulk-copied into the new conversation
/// using `copy` links, which render as dashed arrows in MemoryGraph/MemoryTimeline.
#[tauri::command]
pub async fn branch_conversation(
    state: State<'_, Arc<RwLock<AppState>>>,
    parent_conversation_id: String,
    branch_point_message_id: String,
    new_title: Option<String>,
) -> Result<Conversation, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    // 1. Fetch parent conversation meta
    let parent = get_conversation_by_id(&db, &parent_conversation_id).await?;

    // 2. Walk from branch_point_message_id → root collecting ordered ancestor IDs
    #[derive(sqlx::FromRow)]
    struct MsgRow { id: String, role: String, content: String, parent_id: Option<String> }

    let all_msgs: Vec<MsgRow> = sqlx::query_as(
        "SELECT id, role, content, parent_id FROM messages WHERE conversation_id = ?"
    )
    .bind(&parent_conversation_id)
    .fetch_all(&db)
    .await?;

    let by_id: std::collections::HashMap<String, &MsgRow> =
        all_msgs.iter().map(|m| (m.id.clone(), m)).collect();

    // Walk backward from branch point to root
    let mut path_ids: Vec<String> = Vec::new();
    let mut current: Option<String> = Some(branch_point_message_id.clone());
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = current {
        if !by_id.contains_key(&id) || visited.contains(&id) { break; }
        visited.insert(id.clone());
        path_ids.push(id.clone());
        current = by_id[&id].parent_id.clone();
    }
    path_ids.reverse(); // now root → branch_point

    // 3. Create the new conversation
    let new_conv_id = uuid::Uuid::new_v4().to_string();
    let title = new_title.unwrap_or_else(|| parent.title.clone());

    sqlx::query(
        "INSERT INTO conversations (id, title, character_id, memory_scope, parent_conversation_id, branch_point_message_id)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&new_conv_id)
    .bind(&title)
    .bind(&parent.character_id)
    .bind(&parent.memory_scope)
    .bind(&parent_conversation_id)
    .bind(&branch_point_message_id)
    .execute(&db)
    .await?;

    // 4. Copy messages into new conversation with fresh IDs, re-mapping parent_ids
    let mut old_to_new: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut last_new_id = String::new();

    for old_id in &path_ids {
        let msg = &by_id[old_id];
        let new_msg_id = uuid::Uuid::new_v4().to_string();
        let new_parent_id = msg.parent_id.as_deref()
            .and_then(|pid| old_to_new.get(pid))
            .cloned();

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, parent_id) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&new_msg_id)
        .bind(&new_conv_id)
        .bind(&msg.role)
        .bind(&msg.content)
        .bind(&new_parent_id)
        .execute(&db)
        .await?;

        old_to_new.insert(old_id.clone(), new_msg_id.clone());
        last_new_id = new_msg_id;
    }

    // 5. Set active_message_id to the last copied message
    if !last_new_id.is_empty() {
        sqlx::query("UPDATE conversations SET active_message_id = ? WHERE id = ?")
            .bind(&last_new_id)
            .bind(&new_conv_id)
            .execute(&db)
            .await?;
    }

    // 6. Bulk-copy memories from parent conversation → new conversation as 'copy' links
    //    This makes them appear in MemoryGraph / MemoryTimeline as dashed COPY connectors.
    #[derive(sqlx::FromRow)]
    struct MemRow { id: String, character_id: Option<String>, content: String }

    let parent_mems: Vec<MemRow> = sqlx::query_as(
        "SELECT id, character_id, content FROM memories WHERE conversation_id = ?"
    )
    .bind(&parent_conversation_id)
    .fetch_all(&db)
    .await?;

    for mem in &parent_mems {
        // Create a copy of the memory in the new conversation (parent_id = original mem id)
        let copy_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO memories (id, character_id, conversation_id, content, source, parent_id, version, is_canon)
             VALUES (?, ?, ?, ?, 'auto', ?, 1, 0)"
        )
        .bind(&copy_id)
        .bind(&mem.character_id)
        .bind(&new_conv_id)
        .bind(&mem.content)
        .bind(&mem.id)
        .execute(&db)
        .await?;

        // Create the memory_link record (copy, one_way) — rendered by MemoryGraph as dashed arrow
        let link_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO memory_links (id, source_memory_id, target_conversation_id, link_type, direction, sync_mode, linked_memory_id)
             VALUES (?, ?, ?, 'copy', 'one_way', 'auto', ?)"
        )
        .bind(&link_id)
        .bind(&mem.id)
        .bind(&new_conv_id)
        .bind(&copy_id)
        .execute(&db)
        .await?;
    }

    info!(
        "Branched conversation {} → {} ({} messages, {} memories copied)",
        parent_conversation_id, new_conv_id, path_ids.len(), parent_mems.len()
    );

    get_conversation_by_id(&db, &new_conv_id).await
}

// --- Search ---


use crate::models::conversation::SearchResult;

/// Row returned from the FTS5 search query.
#[derive(sqlx::FromRow)]
struct SearchRow {
    message_id: String,
    conversation_id: String,
    role: String,
    content: String,
    snippet: String,
    conversation_title: String,
    character_name: Option<String>,
    created_at: String,
}

/// Searches message content using SQLite FTS5 full-text search.
///
/// Returns results with highlighted snippets, conversation titles,
/// and character names for display in the search overlay.
#[tauri::command]
pub async fn search_messages(
    state: State<'_, Arc<RwLock<AppState>>>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchResult>, MythicError> {
    let state = state.read().await;
    let limit = limit.unwrap_or(20).min(100);

    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    // Sanitize: wrap each word in double quotes to prevent FTS5 syntax errors
    // from user input like "hello OR world" or unmatched quotes
    let fts_query: String = query
        .split_whitespace()
        .map(|word| {
            let clean: String = word.chars().filter(|c| *c != '"').collect();
            format!("\"{}\"", clean)
        })
        .collect::<Vec<_>>()
        .join(" ");

    let rows = sqlx::query_as::<_, SearchRow>(
        "SELECT
            f.message_id,
            f.conversation_id,
            m.role,
            m.content,
            snippet(messages_fts, 2, '<mark>', '</mark>', '…', 48) AS snippet,
            c.title AS conversation_title,
            ch.name AS character_name,
            m.created_at
         FROM messages_fts f
         JOIN messages m ON m.id = f.message_id
         JOIN conversations c ON c.id = f.conversation_id
         LEFT JOIN characters ch ON ch.id = c.character_id
         WHERE messages_fts MATCH ?
         ORDER BY rank
         LIMIT ?"
    )
    .bind(&fts_query)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    info!("Search '{}' returned {} results", query, rows.len());

    Ok(rows.into_iter().map(|row| {
        let role = match row.role.as_str() {
            "user" => crate::models::conversation::MessageRole::User,
            "assistant" => crate::models::conversation::MessageRole::Assistant,
            "system" => crate::models::conversation::MessageRole::System,
            _ => crate::models::conversation::MessageRole::User,
        };
        SearchResult {
            message_id: row.message_id,
            conversation_id: row.conversation_id,
            role,
            content: row.content,
            snippet: row.snippet,
            conversation_title: row.conversation_title,
            character_name: row.character_name,
            created_at: parse_datetime(&row.created_at),
        }
    }).collect())
}
