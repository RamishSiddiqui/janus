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
    // A browser request carries its own account (see `web::rpc`); only the
    // desktop falls back to the account the desktop window is signed in as.
    if let Ok(web_user) = crate::web::rpc::REQUEST_USER.try_with(|u| u.clone()) {
        let actor = crate::auth::access::resolve_web_actor(&db, &web_user).await?;
        return Ok((db, actor));
    }
    let signed_in = desktop.lock().await.clone();
    let actor = resolve_actor(&db, signed_in.as_deref()).await?;
    Ok((db, actor))
}
