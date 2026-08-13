//! Unified Trash — soft-deleted conversations, characters, and personas in
//! one place, with restore and permanent-delete. Individual `trash_*`/
//! `restore_*` commands live next to each entity's other CRUD commands
//! (`commands::conversations`, `commands::characters`, `commands::personas`);
//! this module only covers the cross-entity list + bulk-empty operations.

use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::info;

use crate::db::characters::CharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::personas::PersonaRepo;
use crate::error::MythicError;
use crate::AppState;

/// A single row in the unified Trash view. `item_type` is one of
/// "conversation" | "character" | "persona" — the frontend uses it to pick
/// the right restore/delete-forever command and display treatment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TrashItem {
    pub id: String,
    pub item_type: String,
    pub name: String,
    pub avatar_path: Option<String>,
    pub deleted_at: String,
}

/// Lists everything currently in the Trash across conversations, characters,
/// and personas — merged and sorted by deleted_at, most recently trashed
/// first.
#[tauri::command]
#[specta::specta]
pub async fn list_trash(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<TrashItem>, MythicError> {
    let state = state.read().await;
    let db = &state.db;

    let conversations = ConversationRepo::list_trashed(db).await?;
    let characters = CharacterRepo::list_trashed(db).await?;
    let personas = PersonaRepo::list_trashed(db).await?;

    let mut items: Vec<TrashItem> = Vec::new();
    for c in conversations {
        items.push(TrashItem {
            id: c.id.id.to_raw(),
            item_type: "conversation".to_string(),
            name: c.title,
            avatar_path: None,
            deleted_at: c.deleted_at.unwrap_or_default(),
        });
    }
    for c in characters {
        items.push(TrashItem {
            id: c.id.id.to_raw(),
            item_type: "character".to_string(),
            name: c.name,
            avatar_path: c.avatar_path,
            deleted_at: c.deleted_at.unwrap_or_default(),
        });
    }
    for p in personas {
        items.push(TrashItem {
            id: p.id.id.to_raw(),
            item_type: "persona".to_string(),
            name: p.name,
            avatar_path: p.avatar_path,
            deleted_at: p.deleted_at.unwrap_or_default(),
        });
    }

    items.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(items)
}

/// Permanently deletes every item currently in the Trash, across all three
/// types. Best-effort per item — one failure is logged and skipped rather
/// than aborting the rest of the sweep.
#[tauri::command]
#[specta::specta]
pub async fn empty_trash(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<(), MythicError> {
    let state = state.read().await;
    let db = &state.db;

    let conversations = ConversationRepo::list_trashed(db).await?;
    for c in &conversations {
        let id = c.id.id.to_raw();
        if let Err(e) = ConversationRepo::delete(db, &id).await {
            tracing::warn!("[empty_trash] Failed to delete conversation {}: {}", id, e);
        }
    }

    let characters = CharacterRepo::list_trashed(db).await?;
    for c in &characters {
        let id = c.id.id.to_raw();
        if let Err(e) = CharacterRepo::delete(db, &id).await {
            tracing::warn!("[empty_trash] Failed to delete character {}: {}", id, e);
        }
    }

    let personas = PersonaRepo::list_trashed(db).await?;
    for p in &personas {
        let id = p.id.id.to_raw();
        if let Err(e) = PersonaRepo::delete(db, &id).await {
            tracing::warn!("[empty_trash] Failed to delete persona {}: {}", id, e);
        }
    }

    info!(
        "Emptied trash: {} conversations, {} characters, {} personas",
        conversations.len(), characters.len(), personas.len()
    );
    Ok(())
}
