pub mod commands;
pub mod db;
pub mod error;
pub mod models;
pub mod providers;

use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Global application state shared across all Tauri command handlers.
///
/// Wrapped in `Arc<RwLock<>>` for safe concurrent access from multiple
/// async command handlers. Registered as Tauri managed state.
pub struct AppState {
    /// SQLite connection pool
    pub db: Pool<Sqlite>,

    /// HTTP client shared across all providers (connection pooling)
    pub http_client: reqwest::Client,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing/logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("mythic=debug,info")),
        )
        .init();

    info!("Starting Mythic v{}", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Resolve the database path in the app data directory
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            let db_path = app_data_dir.join("mythic.db");

            // Initialize the database in a blocking context during setup
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create Tokio runtime");

            let pool = rt.block_on(async {
                db::init_database(&db_path).await
            }).expect("Failed to initialize database");

            // Copy bundled seed avatars to app data dir if not already present
            let avatars_dest = app_data_dir.join("avatars");
            if let Err(e) = std::fs::create_dir_all(&avatars_dest) {
                tracing::warn!("Failed to create avatars directory: {}", e);
            }

            // Try resource dir first (production), then src-tauri/avatars (dev mode)
            let resource_dir = app_handle
                .path()
                .resource_dir()
                .ok()
                .map(|p| p.join("avatars"));
            let dev_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("avatars");
            let avatars_src = match &resource_dir {
                Some(p) if p.exists() => Some(p.as_path()),
                _ if dev_dir.exists() => Some(dev_dir.as_path()),
                _ => None,
            };

            if let Some(src_dir) = avatars_src {
                if let Ok(entries) = std::fs::read_dir(src_dir) {
                    for entry in entries.flatten() {
                        let dest_file = avatars_dest.join(entry.file_name());
                        if !dest_file.exists() {
                            if let Err(e) = std::fs::copy(entry.path(), &dest_file) {
                                tracing::warn!("Failed to copy seed avatar {:?}: {}", entry.file_name(), e);
                            } else {
                                info!("Copied seed avatar: {:?}", entry.file_name());
                            }
                        }
                    }
                }
            }

            // Build a shared HTTP client for all providers
            let http_client = reqwest::Client::builder()
                .user_agent(format!("Mythic/{}", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client");

            // Register global app state
            let state = AppState {
                db: pool,
                http_client,
            };

            app.manage(Arc::new(RwLock::new(state)));

            info!("Mythic initialized successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // App info
            get_app_info,
            // Characters
            commands::characters::create_character,
            commands::characters::get_character,
            commands::characters::list_characters,
            commands::characters::update_character,
            commands::characters::delete_character,
            // Conversations
            commands::conversations::create_conversation,
            commands::conversations::get_conversation,
            commands::conversations::list_conversations,
            commands::conversations::count_conversations,
            commands::conversations::delete_conversation,
            commands::conversations::get_conversation_messages,
            commands::conversations::set_active_message,
            commands::conversations::update_conversation,
            commands::conversations::set_memory_scope,
            // Messages
            commands::messages::create_message,
            commands::messages::update_message,
            commands::messages::delete_message,
            commands::messages::get_message_branch,
            commands::messages::get_message_siblings,
            // Providers
            commands::providers::create_provider,
            commands::providers::get_provider,
            commands::providers::list_providers,
            commands::providers::update_provider,
            commands::providers::delete_provider,
            commands::providers::set_default_provider,
            commands::providers::test_provider_connection,
            commands::providers::list_provider_models,
            // Chat
            commands::chat::send_message,
            commands::chat::regenerate_message,
            commands::chat::generate_raw,
            // Import
            commands::import::import_character_card,
            commands::import::get_avatar_path,
            // Scenes
            commands::scenes::generate_scene,
            commands::scenes::list_scenes,
            commands::scenes::delete_scene,
            commands::scenes::get_scene_path,
            // Lorebook
            commands::lorebook::list_lorebook_entries,
            commands::lorebook::create_lorebook_entry,
            commands::lorebook::toggle_lorebook_entry,
            commands::lorebook::delete_lorebook_entry,
            // Memories
            commands::memories::list_memories,
            commands::memories::create_memory,
            commands::memories::delete_memory,
            // Search
            commands::conversations::search_messages,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Mythic");
}

/// Returns basic app information for the frontend.
#[tauri::command]
fn get_app_info() -> serde_json::Value {
    serde_json::json!({
        "name": "Mythic",
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
    })
}
