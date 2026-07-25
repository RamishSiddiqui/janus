pub mod commands;
pub mod context;
pub mod db;
pub mod error;
pub mod models;
pub mod providers;

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
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
    /// SurrealDB embedded database connection
    pub db: Surreal<Db>,

    /// HTTP client shared across all providers (connection pooling)
    pub http_client: reqwest::Client,
}

/// Builds the tauri-specta command registry — the single source of truth
/// for which commands get IPC bindings generated for the frontend. Add a
/// command here (and give it `#[specta::specta]`) to include it in the
/// generated `src/lib/services/bindings.ts`; the same list also becomes the
/// Tauri invoke handler, so a command registered here doesn't need a
/// separate `tauri::generate_handler!` entry.
pub(crate) fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_app_info,
            commands::characters::create_character,
            commands::characters::get_character,
            commands::characters::list_characters,
            commands::characters::update_character,
            commands::characters::delete_character,
            commands::character_state::get_character_state,
            commands::character_state::upsert_character_state,
            commands::conversations::create_conversation,
            commands::conversations::get_conversation,
            commands::conversations::list_conversations,
            commands::conversations::count_conversations,
            commands::conversations::delete_conversation,
            commands::conversations::get_conversation_messages,
            commands::conversations::set_active_message,
            commands::conversations::update_conversation,
            commands::conversations::set_memory_scope,
            commands::conversations::branch_conversation,
            commands::conversations::search_messages,
            commands::messages::create_message,
            commands::messages::update_message,
            commands::messages::delete_message,
            commands::messages::get_message_branch,
            commands::messages::get_message_siblings,
            commands::providers::create_provider,
            commands::providers::get_provider,
            commands::providers::list_providers,
            commands::providers::update_provider,
            commands::providers::delete_provider,
            commands::providers::set_default_provider,
            commands::providers::test_provider_connection,
            commands::providers::list_provider_models,
            commands::providers::list_all_models,
            commands::providers::list_embedding_models,
            commands::providers::toggle_model_enabled,
            commands::providers::list_enabled_models,
            commands::lorebook::list_lorebook_entries,
            commands::lorebook::create_lorebook_entry,
            commands::lorebook::toggle_lorebook_entry,
            commands::lorebook::delete_lorebook_entry,
            commands::memories::list_memories,
            commands::memories::create_memory,
            commands::memories::update_memory,
            commands::memories::set_memory_importance,
            commands::memories::delete_memory,
            commands::memories::promote_to_canon,
            commands::memories::share_memory,
            commands::memories::unlink_memory,
            commands::memories::get_memory_graph,
            commands::scenes::generate_scene,
            commands::scenes::list_scenes,
            commands::scenes::delete_scene,
            commands::scenes::get_scene_path,
            commands::scene_states::get_scene_state,
            commands::scene_states::upsert_scene_state,
            commands::scene_states::delete_scene_state,
            commands::conversation_characters::list_conversation_characters,
            commands::conversation_characters::add_conversation_character,
            commands::conversation_characters::remove_conversation_character,
            commands::conversation_characters::update_character_talkativeness,
            commands::conversation_characters::toggle_character_active,
            commands::embeddings::get_embedding_index_status,
            commands::embeddings::rebuild_embedding_index,
            commands::embeddings::backfill_missing_embeddings,
            commands::import::import_character_card,
            commands::import::get_avatar_path,
        ])
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

    let specta_builder = specta_builder();

    #[cfg(debug_assertions)]
    specta_builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/lib/services/bindings.ts",
        )
        .expect("Failed to export TypeScript bindings");

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

            // Initialize the database using Tauri's own async runtime.
            // IMPORTANT: We must NOT create a temporary tokio::runtime::Runtime here.
            // SurrealDB spawns internal async tasks on the current runtime — if that
            // runtime is dropped (as a temporary one would be at end of setup()),
            // those tasks die and the Surreal<Db> handle becomes a dead channel.
            let db = tauri::async_runtime::block_on(async {
                db::init_database(&app_data_dir).await
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
                // Only set a connect timeout — NOT an overall timeout.
                // Streaming SSE responses can legitimately run for minutes;
                // an overall timeout would kill them mid-stream.
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client");

            // Register global app state
            let state = AppState {
                db,
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
            commands::conversations::branch_conversation,
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
            commands::providers::list_all_models,
            commands::providers::list_embedding_models,
            commands::providers::toggle_model_enabled,
            commands::providers::list_enabled_models,
            // Chat
            commands::chat::send_message,
            commands::chat::retry_failed_message,
            commands::chat::regenerate_message,
            commands::chat::generate_raw,
            commands::chat::get_context_stats,
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
            commands::memories::update_memory,
            commands::memories::set_memory_importance,
            commands::memories::delete_memory,
            commands::memories::promote_to_canon,
            commands::memories::share_memory,
            commands::memories::unlink_memory,
            commands::memories::get_memory_graph,
            // Search
            commands::conversations::search_messages,
            // Character State
            commands::character_state::get_character_state,
            commands::character_state::upsert_character_state,
            // Scene State
            commands::scene_states::get_scene_state,
            commands::scene_states::upsert_scene_state,
            commands::scene_states::delete_scene_state,
            // Conversation Characters
            commands::conversation_characters::list_conversation_characters,
            commands::conversation_characters::add_conversation_character,
            commands::conversation_characters::remove_conversation_character,
            commands::conversation_characters::update_character_talkativeness,
            commands::conversation_characters::toggle_character_active,
            // Embeddings
            commands::embeddings::get_embedding_index_status,
            commands::embeddings::rebuild_embedding_index,
            commands::embeddings::backfill_missing_embeddings,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Mythic");
}

/// Basic app metadata surfaced to the frontend (About screen, etc.).
#[derive(serde::Serialize, specta::Type)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

/// Returns basic app information for the frontend.
#[tauri::command]
#[specta::specta]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Mythic".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
    }
}
