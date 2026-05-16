use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::{MythicError, validate_required_string};
use crate::models::character::Character;
use crate::AppState;

/// Creates a new character from a Character Card V2 payload.
#[tauri::command]
pub async fn create_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    name: String,
    data: serde_json::Value,
) -> Result<Character, MythicError> {
    validate_required_string("Character name", &name, 200)?;

    let state = state.read().await;
    let id = Uuid::new_v4().to_string();

    let data_str = serde_json::to_string(&data)?;

    sqlx::query(
        "INSERT INTO characters (id, name, spec, data) VALUES (?, ?, 'chara_card_v2', ?)"
    )
    .bind(&id)
    .bind(&name)
    .bind(&data_str)
    .execute(&state.db)
    .await?;

    info!("Created character: {} ({})", name, id);

    get_character_by_id(&state.db, &id).await
}

/// Retrieves a single character by ID.
#[tauri::command]
pub async fn get_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<Character, MythicError> {
    let state = state.read().await;
    get_character_by_id(&state.db, &id).await
}

/// Lists all characters, ordered by most recently updated.
#[tauri::command]
pub async fn list_characters(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<Character>, MythicError> {
    let state = state.read().await;

    let rows = sqlx::query_as::<_, CharacterRow>(
        "SELECT id, name, spec, data, avatar_path, created_at, updated_at
         FROM characters ORDER BY updated_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().map(Into::into).collect())
}

/// Updates an existing character's data.
#[tauri::command]
pub async fn update_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
    name: Option<String>,
    data: Option<serde_json::Value>,
    avatar_path: Option<String>,
) -> Result<Character, MythicError> {
    if let Some(ref name) = name {
        validate_required_string("Character name", name, 200)?;
    }

    let state = state.read().await;

    // Verify the character exists
    get_character_by_id(&state.db, &id).await?;

    if let Some(ref name) = name {
        sqlx::query("UPDATE characters SET name = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(name)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref data) = data {
        let data_str = serde_json::to_string(data)?;
        sqlx::query("UPDATE characters SET data = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(&data_str)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref avatar_path) = avatar_path {
        sqlx::query("UPDATE characters SET avatar_path = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(avatar_path)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    info!("Updated character: {}", id);
    get_character_by_id(&state.db, &id).await
}

/// Deletes a character by ID. Cascades to lorebook entries.
#[tauri::command]
pub async fn delete_character(
    state: State<'_, Arc<RwLock<AppState>>>,
    id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;

    let result = sqlx::query("DELETE FROM characters WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MythicError::NotFound(format!("Character not found: {}", id)));
    }

    // Clean up orphan conversations and their messages
    sqlx::query(
        "DELETE FROM messages WHERE conversation_id IN (SELECT id FROM conversations WHERE character_id = ?)"
    )
    .bind(&id)
    .execute(&state.db)
    .await?;

    sqlx::query("DELETE FROM conversations WHERE character_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    // Clean up lorebook entries
    sqlx::query("DELETE FROM lorebook_entries WHERE character_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    info!("Deleted character and related data: {}", id);
    Ok(())
}

// --- Internal helpers ---

/// SQLite row mapping for characters.
#[derive(sqlx::FromRow)]
struct CharacterRow {
    id: String,
    name: String,
    spec: String,
    data: String,
    avatar_path: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<CharacterRow> for Character {
    fn from(row: CharacterRow) -> Self {
        Character {
            id: row.id,
            name: row.name,
            spec: row.spec,
            data: row.data,
            avatar_path: row.avatar_path,
            created_at: chrono::DateTime::parse_from_str(&row.created_at, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_str(&row.updated_at, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }
}

async fn get_character_by_id(
    db: &sqlx::Pool<sqlx::Sqlite>,
    id: &str,
) -> Result<Character, MythicError> {
    let row = sqlx::query_as::<_, CharacterRow>(
        "SELECT id, name, spec, data, avatar_path, created_at, updated_at
         FROM characters WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))?;

    Ok(row.into())
}
