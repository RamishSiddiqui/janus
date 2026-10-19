//! Delivers the app's events (`chat-stream`, `tts-chunk`, ...) to the right
//! account's screens (issue #94, phase 3).
//!
//! Tauri's `app.emit` is app-wide: every open window receives it. Once more
//! than one account is in use that is wrong, since one account's streamed
//! reply (or the audio that speaks it) would reach another's screen. So the
//! emit call sites use [`EmitScoped::emit_scoped`] instead. Each event is
//! resolved to its owning account, then sent
//! - to that account's browser sessions (through the SSE bus), and
//! - to the desktop window only if it belongs to the account the desktop is
//!   signed in as (or belongs to nobody, or no accounts exist yet).
//!
//! Events go through one queue and one task, so a chat stream's chunks stay in
//! order.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::db::users::OwnershipRepo;
use crate::AppState;

/// Events with no account-specific data that every signed-in browser may
/// receive (they only tell the UI to refresh something it already shows).
const PUBLIC_EVENTS: &[&str] = &["embedding_updated"];

/// An event as it travels to browsers.
#[derive(Debug, Clone)]
pub struct BusEvent {
    pub name: String,
    pub payload: Value,
    /// The account it belongs to; `None` for events that belong to nobody.
    pub owner: Option<String>,
}

impl BusEvent {
    pub fn visible_to(&self, user_id: &str) -> bool {
        match &self.owner {
            Some(owner) => owner == user_id,
            None => PUBLIC_EVENTS.contains(&self.name.as_str()),
        }
    }
}

pub type Bus = broadcast::Sender<Arc<BusEvent>>;

pub fn new_bus() -> Bus {
    broadcast::channel(512).0
}

static QUEUE: OnceLock<mpsc::UnboundedSender<(String, Value)>> = OnceLock::new();

/// Starts the task that routes events. Call once, after `AppState` is managed.
pub fn start(app: AppHandle, bus: Bus) {
    let (tx, mut rx) = mpsc::unbounded_channel::<(String, Value)>();
    if QUEUE.set(tx).is_err() {
        return; // already started
    }
    tauri::async_runtime::spawn(async move {
        let mut cache = OwnerCache::default();
        while let Some((name, payload)) = rx.recv().await {
            let owner = cache.owner_of_event(&app, &name, &payload).await;
            deliver(&app, &bus, name, payload, owner).await;
        }
    });
}

async fn deliver(app: &AppHandle, bus: &Bus, name: String, payload: Value, owner: Option<String>) {
    // Desktop window: its own account's events, and anything unowned.
    let desktop_user = match app.try_state::<Arc<RwLock<AppState>>>() {
        Some(state) => {
            let slot = state.read().await.desktop_user.clone();
            let id = slot.lock().await.clone();
            id
        }
        None => None,
    };
    let desktop_gets_it = match (&owner, &desktop_user) {
        (None, _) => true,
        (Some(_), None) => true, // no accounts yet, or signed out: legacy behaviour
        (Some(o), Some(d)) => o == d,
    };
    if desktop_gets_it {
        let _ = app.emit(&name, payload.clone());
    }
    // Browsers: nobody listening is fine.
    let _ = bus.send(Arc::new(BusEvent {
        name,
        payload,
        owner,
    }));
}

pub trait EmitScoped {
    fn emit_scoped<S: Serialize + Clone>(&self, event: &str, payload: S) -> tauri::Result<()>;
}

impl<R: Runtime> EmitScoped for AppHandle<R> {
    fn emit_scoped<S: Serialize + Clone>(&self, event: &str, payload: S) -> tauri::Result<()> {
        match QUEUE.get() {
            Some(queue) => {
                let value = serde_json::to_value(&payload)?;
                let _ = queue.send((event.to_string(), value));
                Ok(())
            }
            // Router not started (tests, very early startup): plain emit.
            None => self.emit(event, payload),
        }
    }
}

/// Which account an event belongs to, found from the ids in its payload.
#[derive(Default)]
struct OwnerCache {
    conversations: HashMap<String, Option<String>>,
    messages: HashMap<String, Option<String>>,
}

impl OwnerCache {
    async fn owner_of_event(
        &mut self,
        app: &AppHandle,
        name: &str,
        payload: &Value,
    ) -> Option<String> {
        let state = app.try_state::<Arc<RwLock<AppState>>>()?;
        let db = state.read().await.db.clone();

        let text = |keys: &[&str]| -> Option<String> {
            keys.iter()
                .find_map(|k| payload.get(*k).and_then(|v| v.as_str()))
                .map(str::to_string)
        };

        if let Some(conv) = text(&["conversation_id", "conversationId"]) {
            return self.conversation_owner(&db, &conv).await;
        }
        if let Some(msg) = text(&["message_id", "messageId"]) {
            return self.message_owner(&db, &msg).await;
        }
        if let Some(character) = text(&["character_id", "characterId"]) {
            return match OwnershipRepo::owner_of(&db, "characters", &character).await {
                Ok(Some(o)) if !o.is_empty() => Some(o),
                _ => None,
            };
        }
        let _ = name;
        None
    }

    async fn conversation_owner(
        &mut self,
        db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
        id: &str,
    ) -> Option<String> {
        if let Some(hit) = self.conversations.get(id) {
            return hit.clone();
        }
        let owner = match OwnershipRepo::owner_of(db, "conversations", id).await {
            Ok(Some(o)) if !o.is_empty() => Some(o),
            _ => None,
        };
        if self.conversations.len() > 4096 {
            self.conversations.clear();
        }
        self.conversations.insert(id.to_string(), owner.clone());
        owner
    }

    async fn message_owner(
        &mut self,
        db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
        id: &str,
    ) -> Option<String> {
        if let Some(hit) = self.messages.get(id) {
            return hit.clone();
        }
        let owner = match OwnershipRepo::owner_via_conversation(db, "messages", id).await {
            Ok(Some(o)) if !o.is_empty() => Some(o),
            _ => None,
        };
        // A message that doesn't exist yet may appear a moment later: don't cache a miss.
        if owner.is_some() {
            if self.messages.len() > 8192 {
                self.messages.clear();
            }
            self.messages.insert(id.to_string(), owner.clone());
        }
        owner
    }
}
