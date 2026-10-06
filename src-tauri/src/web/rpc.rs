//! Calls the app's existing Tauri commands from a browser (issue #94, phase 3).
//!
//! A command is exposed by listing it in a family table (`web/rpc_*.rs`). The
//! browser POSTs `/api/rpc/{command}` with the same camelCase JSON arguments
//! the desktop UI sends to `invoke()`. The handler resolves the session, then
//! runs the *same* command function the desktop runs, with the signed-in
//! account carried in a task-local that `commands::actor::acting` reads. So
//! every ownership check written for the desktop applies unchanged, and there
//! is no second copy of any command's logic.
//!
//! Commands that read or write paths on the host machine (file pickers,
//! imports from a path, backups) are deliberately NOT listed here; the
//! browser gets purpose-built endpoints for those instead.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;

use super::util::{ActiveUser, ApiError};
use super::WebState;
use crate::error::MythicError;

tokio::task_local! {
    /// The account a browser request is acting as. Unset on the desktop path.
    pub static REQUEST_USER: String;
}

/// A table lookup result: `None` means "not in this table".
pub type Called = Option<Result<Value, MythicError>>;
pub type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// How the server reaches the command functions. Implemented for the real
/// app handle, and for a mock one in tests.
pub trait Rpc: Send + Sync {
    fn call(&self, name: String, args: Value) -> BoxFut<Result<Value, MythicError>>;
}

pub fn parse_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, MythicError> {
    // The desktop sends no body for a command with no arguments.
    let args = if args.is_null() {
        Value::Object(Default::default())
    } else {
        args
    };
    serde_json::from_value(args)
        .map_err(|e| MythicError::Validation(format!("Invalid arguments: {e}")))
}

pub fn to_value<T: serde::Serialize>(v: T) -> Result<Value, MythicError> {
    serde_json::to_value(v).map_err(MythicError::from)
}

/// One match arm: parse the JSON args, call the command, serialize the result.
///
/// `$pre` lists the leading injected parameters in order: `state` (a
/// `State<'_, Arc<RwLock<AppState>>>`) and/or `app` (an `AppHandle`).
#[macro_export]
macro_rules! rpc_call {
    ($app:expr, $args:expr, $func:path, [$($pre:ident),*] ($($a:ident : $t:ty),*)) => {{
        {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args { $($a: $t,)* }
            let parsed: Result<Args, $crate::error::MythicError> = $crate::web::rpc::parse_args($args);
            match parsed {
                Err(e) => Err(e),
                Ok(Args { $($a),* }) => {
                    let result = $func($($crate::rpc_call!(@pre $app, $pre),)* $($a),*).await;
                    result.and_then($crate::web::rpc::to_value)
                }
            }
        }
    }};
    (@pre $app:expr, state) => { tauri::Manager::state(&$app) };
    (@pre $app:expr, app) => { $app.clone() };
}

/// Builds `pub async fn call(app, name, args) -> Called` from a list of
/// `"name" => path [injected] (arg: Type, ...)` entries. Generic over the
/// runtime, so it only takes commands that need `State`, not a Wry `AppHandle`.
#[macro_export]
macro_rules! rpc_family {
    ($($name:literal => $func:path [$($pre:ident),*] ($($a:ident : $t:ty),*);)*) => {
        #[allow(unused_variables)]
        pub async fn call<R: tauri::Runtime>(
            app: &tauri::AppHandle<R>,
            name: &str,
            args: serde_json::Value,
        ) -> $crate::web::rpc::Called {
            let app = app.clone();
            let result = match name {
                $($name => $crate::rpc_call!(app, args, $func, [$($pre),*] ($($a : $t),*)),)*
                _ => return None,
            };
            Some(result)
        }
    };
}

/// Like `rpc_family!`, for commands that take the real window's `AppHandle`
/// (so they can only run against the Wry runtime, not the test mock). Generates
/// `call_wry`.
#[macro_export]
macro_rules! rpc_family_wry {
    ($($name:literal => $func:path [$($pre:ident),*] ($($a:ident : $t:ty),*);)*) => {
        #[allow(unused_variables)]
        pub async fn call_wry(
            app: &tauri::AppHandle,
            name: &str,
            args: serde_json::Value,
        ) -> $crate::web::rpc::Called {
            let app = app.clone();
            let result = match name {
                $($name => $crate::rpc_call!(app, args, $func, [$($pre),*] ($($a : $t),*)),)*
                _ => return None,
            };
            Some(result)
        }
    };
}

/// The real implementation: the app handle plus the family tables.
pub struct AppRpc(pub tauri::AppHandle);

impl Rpc for AppRpc {
    fn call(&self, name: String, args: Value) -> BoxFut<Result<Value, MythicError>> {
        let app = self.0.clone();
        Box::pin(async move { super::rpc_tables::dispatch(&app, &name, args).await })
    }
}

/// `POST /api/rpc/{name}`
pub async fn handle(
    State(st): State<WebState>,
    ActiveUser(authed): ActiveUser,
    Path(name): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let Some(rpc) = st.rpc.clone() else {
        return Err(ApiError(MythicError::NotFound(
            "Commands aren't available here.".to_string(),
        )));
    };
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let value = REQUEST_USER
        .scope(authed.user.id.clone(), rpc.call(name, args))
        .await?;
    Ok(Json(value))
}

#[allow(dead_code)]
pub fn shared<T: Rpc + 'static>(rpc: T) -> Arc<dyn Rpc> {
    Arc::new(rpc)
}
