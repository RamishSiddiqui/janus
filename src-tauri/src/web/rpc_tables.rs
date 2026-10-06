//! The list of command families the browser can call. Each family lives in its
//! own `rpc_*.rs` file so they can be extended independently.
//!
//! A family file has `rpc_family!` (commands that only need `State`; works on
//! any runtime, so the tests drive it through a mock app) and/or
//! `rpc_family_wry!` (commands that take the real window's `AppHandle`).

use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::rpc::Called;
use crate::error::MythicError;

macro_rules! try_generic {
    ($app:expr, $name:expr, $args:expr, $($family:path),+ $(,)?) => {
        $(
            if let Some(r) = $family($app, $name, $args.clone()).await {
                return Some(r);
            }
        )+
    };
}

pub async fn dispatch_generic<R: Runtime>(app: &AppHandle<R>, name: &str, args: Value) -> Called {
    try_generic!(
        app,
        name,
        args,
        super::rpc_characters::call,
        super::rpc_conversations::call,
        super::rpc_providers::call,
        super::rpc_chat::call,
    );
    None
}

/// Every family, including commands that need the real window's `AppHandle`.
pub async fn dispatch(app: &AppHandle, name: &str, args: Value) -> Result<Value, MythicError> {
    if let Some(r) = dispatch_generic(app, name, args.clone()).await {
        return r;
    }
    if let Some(r) = super::rpc_conversations::call_wry(app, name, args.clone()).await {
        return r;
    }
    if let Some(r) = super::rpc_providers::call_wry(app, name, args.clone()).await {
        return r;
    }
    if let Some(r) = super::rpc_chat::call_wry(app, name, args.clone()).await {
        return r;
    }
    Err(MythicError::NotFound(format!("No such command: {name}")))
}
