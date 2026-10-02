//! Who is acting, and whether they may touch a given row (issue #94).
//!
//! Root entities carry an `owner_id`; children are checked through their
//! root. Before any account exists the app runs in `Legacy` mode and nothing
//! is filtered, exactly as it did before accounts.

use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::db::users::{OwnershipRepo, UserRepo};
use crate::error::MythicError;
use crate::models::user::{Role, UserInfo};

#[derive(Debug, Clone)]
pub enum Actor {
    /// No accounts exist yet: everything is visible and new rows are unowned.
    Legacy,
    User(UserInfo),
}

impl Actor {
    /// The `owner_id` to stamp on new rows and to filter lists by.
    pub fn owner(&self) -> &str {
        match self {
            Actor::Legacy => "",
            Actor::User(u) => &u.id,
        }
    }

    /// For list queries: `None` (no filter) in legacy mode, otherwise the
    /// account whose rows to return.
    pub fn owner_filter(&self) -> Option<&str> {
        match self {
            Actor::Legacy => None,
            Actor::User(u) => Some(&u.id),
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Actor::Legacy)
    }

    pub fn is_admin(&self) -> bool {
        match self {
            Actor::Legacy => true,
            Actor::User(u) => u.role == Role::Admin,
        }
    }

    pub fn require_admin(&self) -> Result<(), MythicError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(MythicError::Unauthorized(
                "Only the admin can do that.".to_string(),
            ))
        }
    }
}

/// Resolves who is acting from the signed-in account id (the desktop's
/// `desktop_user`, later a session's user). `None` means nobody is signed in,
/// which is fine only while no accounts exist.
pub async fn resolve_actor(
    db: &Surreal<Db>,
    signed_in: Option<&str>,
) -> Result<Actor, MythicError> {
    if let Some(id) = signed_in {
        if let Some(user) = UserRepo::get(db, id).await? {
            return Ok(Actor::User(UserInfo::from(&user)));
        }
    }
    if UserRepo::count(db).await? == 0 {
        Ok(Actor::Legacy)
    } else {
        Err(MythicError::Unauthorized("Sign in first.".to_string()))
    }
}

/// Fails with the same "not found" for a row that doesn't exist and one that
/// belongs to someone else, so ids can't be probed across accounts.
pub async fn ensure_owned(
    db: &Surreal<Db>,
    actor: &Actor,
    table: &str,
    id: &str,
) -> Result<(), MythicError> {
    if actor.is_legacy() {
        return Ok(());
    }
    match OwnershipRepo::owner_of(db, table, id).await? {
        Some(owner) if owner == actor.owner() => Ok(()),
        _ => Err(MythicError::NotFound(format!("{table} not found: {id}"))),
    }
}

/// Stamps a row the actor just created with their id. A no-op in legacy mode
/// (rows stay unowned until the first account claims them).
pub async fn stamp_owner(
    db: &Surreal<Db>,
    actor: &Actor,
    table: &str,
    id: &str,
) -> Result<(), MythicError> {
    if actor.is_legacy() {
        return Ok(());
    }
    OwnershipRepo::set_owner(db, table, id, actor.owner()).await
}

/// Gives a row created by a background pipeline (NPC detection, ...) the
/// same owner as the conversation it came from.
pub async fn inherit_owner_from_conversation(
    db: &Surreal<Db>,
    conversation_id: &str,
    table: &str,
    id: &str,
) -> Result<(), MythicError> {
    match OwnershipRepo::owner_of(db, "conversations", conversation_id).await? {
        Some(owner) if !owner.is_empty() => OwnershipRepo::set_owner(db, table, id, &owner).await,
        _ => Ok(()),
    }
}

/// A message or scene is yours if its conversation is.
pub async fn ensure_child_of_conversation(
    db: &Surreal<Db>,
    actor: &Actor,
    table: &str,
    id: &str,
) -> Result<(), MythicError> {
    if actor.is_legacy() {
        return Ok(());
    }
    match OwnershipRepo::owner_via_conversation(db, table, id).await? {
        Some(owner) if owner == actor.owner() => Ok(()),
        _ => Err(MythicError::NotFound(format!("{table} not found: {id}"))),
    }
}
pub async fn ensure_message(db: &Surreal<Db>, a: &Actor, id: &str) -> Result<(), MythicError> {
    ensure_child_of_conversation(db, a, "messages", id).await
}
pub async fn ensure_scene(db: &Surreal<Db>, a: &Actor, id: &str) -> Result<(), MythicError> {
    ensure_child_of_conversation(db, a, "scenes", id).await
}

pub async fn ensure_character(db: &Surreal<Db>, a: &Actor, id: &str) -> Result<(), MythicError> {
    ensure_owned(db, a, "characters", id).await
}
pub async fn ensure_persona(db: &Surreal<Db>, a: &Actor, id: &str) -> Result<(), MythicError> {
    ensure_owned(db, a, "personas", id).await
}
pub async fn ensure_conversation(db: &Surreal<Db>, a: &Actor, id: &str) -> Result<(), MythicError> {
    ensure_owned(db, a, "conversations", id).await
}
pub async fn ensure_provider(db: &Surreal<Db>, a: &Actor, id: &str) -> Result<(), MythicError> {
    ensure_owned(db, a, "provider_configs", id).await
}

/// A scene media file (`scenes/<id>.png`) is yours if the scene row that
/// points at it belongs to one of your conversations. Missing and not-yours
/// give the same "not found".
pub async fn ensure_scene_file(
    db: &Surreal<Db>,
    actor: &Actor,
    file_relative: &str,
) -> Result<(), MythicError> {
    if actor.is_legacy() {
        return Ok(());
    }
    let mut result = db
        .query("SELECT conversation_id.owner_id AS owner_id FROM scenes WHERE file_path = $p")
        .bind(("p", file_relative.to_string()))
        .await?;
    let raw: Vec<surrealdb::types::Value> = result.take(0)?;
    let rows: Vec<serde_json::Value> = crate::db::value_bridge::from_value_vec(raw)?;
    let owned = rows
        .iter()
        .any(|v| v.get("owner_id").and_then(|o| o.as_str()) == Some(actor.owner()));
    if owned {
        Ok(())
    } else {
        Err(MythicError::NotFound(format!(
            "scene file not found: {file_relative}"
        )))
    }
}

/// The owner filter to use for work done on behalf of a conversation but
/// outside any command (RAG lookups, summaries, NPC detection): the
/// conversation's owner when it has one, otherwise `None` (legacy mode).
pub async fn owner_filter_for_conversation(
    db: &Surreal<Db>,
    conversation_id: &str,
) -> Result<Option<String>, MythicError> {
    Ok(
        OwnershipRepo::owner_of(db, "conversations", conversation_id)
            .await?
            .filter(|o| !o.is_empty()),
    )
}

/// Like `owner_filter_for_conversation`, for any owned row (e.g. the memory
/// a background embedding job was started for).
pub async fn owner_filter_for_row(
    db: &Surreal<Db>,
    table: &str,
    id: &str,
) -> Result<Option<String>, MythicError> {
    Ok(OwnershipRepo::owner_of(db, table, id)
        .await?
        .filter(|o| !o.is_empty()))
}
