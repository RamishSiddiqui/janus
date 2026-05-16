use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

use crate::models::lorebook::LorebookEntry;

/// Lists all lorebook entries for a character (plus global entries).
#[tauri::command]
pub async fn list_lorebook_entries(
    db: State<'_, SqlitePool>,
    character_id: String,
) -> Result<Vec<LorebookEntry>, String> {
    let rows: Vec<(String, Option<String>, String, String, bool, bool, i32, i32, Option<String>)> =
        sqlx::query_as(
            "SELECT id, character_id, keys, content, enabled, always_active, priority, insertion_order, name
             FROM lorebook_entries
             WHERE character_id = ? OR character_id IS NULL
             ORDER BY priority DESC, insertion_order ASC"
        )
        .bind(&character_id)
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let entries = rows
        .into_iter()
        .map(|(id, character_id, keys_json, content, enabled, always_active, priority, insertion_order, name)| {
            let keys: Vec<String> = serde_json::from_str(&keys_json).unwrap_or_default();
            LorebookEntry {
                id,
                character_id,
                keys,
                content,
                enabled,
                always_active,
                priority,
                insertion_order,
                name,
            }
        })
        .collect();

    Ok(entries)
}

/// Creates a new lorebook entry.
#[tauri::command]
pub async fn create_lorebook_entry(
    db: State<'_, SqlitePool>,
    character_id: Option<String>,
    name: String,
    keys: Vec<String>,
    content: String,
    always_active: bool,
) -> Result<LorebookEntry, String> {
    let id = Uuid::new_v4().to_string();
    let keys_json = serde_json::to_string(&keys).map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO lorebook_entries (id, character_id, keys, content, enabled, always_active, priority, insertion_order, name)
         VALUES (?, ?, ?, ?, 1, ?, 10, 100, ?)"
    )
    .bind(&id)
    .bind(&character_id)
    .bind(&keys_json)
    .bind(&content)
    .bind(always_active)
    .bind(&name)
    .execute(db.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(LorebookEntry {
        id,
        character_id,
        keys,
        content,
        enabled: true,
        always_active,
        priority: 10,
        insertion_order: 100,
        name: Some(name),
    })
}

/// Toggles a lorebook entry's enabled state.
#[tauri::command]
pub async fn toggle_lorebook_entry(
    db: State<'_, SqlitePool>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    sqlx::query("UPDATE lorebook_entries SET enabled = ? WHERE id = ?")
        .bind(enabled)
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes a lorebook entry.
#[tauri::command]
pub async fn delete_lorebook_entry(
    db: State<'_, SqlitePool>,
    id: String,
) -> Result<(), String> {
    sqlx::query("DELETE FROM lorebook_entries WHERE id = ?")
        .bind(&id)
        .execute(db.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
