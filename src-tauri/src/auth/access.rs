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
