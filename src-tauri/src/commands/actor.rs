//! One call that tells a command who is acting (issue #94). Every command
//! that touches user data starts with `let (db, actor) = acting(&state).await?;`
//! then either filters by `actor.owner()` or calls an `ensure_*` check from
//! `auth::access` on the id it was given.

use std::sync::Arc;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tauri::State;
use tokio::sync::RwLock;

use crate::auth::access::{resolve_actor, Actor};
use crate::error::MythicError;
use crate::AppState;

pub async fn acting(
    state: &State<'_, Arc<RwLock<AppState>>>,
) -> Result<(Surreal<Db>, Actor), MythicError> {
    let (db, desktop) = {
        let s = state.read().await;
        (s.db.clone(), s.desktop_user.clone())
    };
    let signed_in = desktop.lock().await.clone();
    let actor = resolve_actor(&db, signed_in.as_deref()).await?;
    Ok((db, actor))
}
