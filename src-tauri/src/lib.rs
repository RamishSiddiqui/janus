// Tauri commands routinely need one param per meaningful argument (they're
// the IPC surface, not internal helpers free to take a params struct), and
// `MythicError` being a moderately large enum is an accepted tradeoff across
// the whole codebase, not a hot-path perf concern for a desktop app. Neither
// represents a real bug — allowed crate-wide rather than peppering ~20 call
// sites with per-function allows.
#![allow(
    clippy::too_many_arguments,
    clippy::result_large_err,
    clippy::type_complexity
)]

pub mod commands;
pub mod context;
pub mod db;
pub mod error;
pub mod models;
pub mod providers;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tauri::Manager;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// A single in-flight streaming (or non-streaming) generation, tracked so
/// `cancel_generation` can abort it and — for the streaming case — persist
/// whatever content had already streamed rather than losing it.
pub struct GenerationHandle {
    pub abort: tokio::task::AbortHandle,
    /// `Some` for a streaming generation (updated with each delta as it
    /// arrives); `None` for non-streaming, which has no partial content to
    /// preserve on cancel.
    pub partial: Option<Arc<StdMutex<String>>>,
    pub assistant_message_id: String,
}

/// Global application state shared across all Tauri command handlers.
///
/// Wrapped in `Arc<RwLock<>>` for safe concurrent access from multiple
/// async command handlers. Registered as Tauri managed state.
pub struct AppState {
    /// SurrealDB embedded database connection
    pub db: Surreal<Db>,

    /// HTTP client shared across all providers (connection pooling)
    pub http_client: reqwest::Client,

    /// In-flight generations keyed by conversation_id — only one generation
    /// can be active per conversation at a time (the frontend gates sending
    /// another message while `isStreaming` is true), so conversation_id is
    /// a sufficient key.
    pub active_generations: Arc<AsyncMutex<HashMap<String, GenerationHandle>>>,

    /// In-flight AI Horde scene generations keyed by conversation_id. Unlike
    /// chat generation (a spawned task we can `.abort()` outright), the
    /// scene poll loop runs inline in the command's own async fn — so
    /// cancellation is a flag the loop checks each tick instead, letting it
    /// still issue a best-effort DELETE to free the worker slot before
    /// returning a "cancelled" error.
    pub active_scene_generations:
        Arc<AsyncMutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>>,
}

/// Builds the tauri-specta command registry — the single source of truth
/// for which commands get IPC bindings generated for the frontend. Add a
/// command here (and give it `#[specta::specta]`) to include it in the
/// generated `src/lib/services/bindings.ts`; the same list also becomes the
/// Tauri invoke handler, so a command registered here doesn't need a
/// separate `tauri::generate_handler!` entry.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        get_app_info,
        commands::characters::create_character,
        commands::characters::get_character,
        commands::characters::list_characters,
        commands::characters::update_character,
        commands::characters::delete_character,
        commands::characters::trash_character,
        commands::characters::restore_character,
        commands::characters::upload_character_avatar,
        commands::character_state::get_character_state,
        commands::character_state::upsert_character_state,
        commands::character_state::set_message_emotional_snapshot,
        commands::conversations::create_conversation,
        commands::conversations::get_conversation,
        commands::conversations::list_conversations,
        commands::conversations::count_conversations,
        commands::conversations::delete_conversation,
        commands::conversations::trash_conversation,
        commands::conversations::restore_conversation,
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
        commands::lorebook::update_lorebook_entry,
        commands::lorebook::import_character_book_entries,
        commands::lorebook::generate_character_lorebook,
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
        commands::scenes::generate_video_scene,
        commands::scenes::list_scene_cast_members,
        commands::scenes::cancel_scene_generation,
        commands::scenes::list_scenes,
        commands::scenes::delete_scene,
        commands::scenes::get_scene_path,
        commands::scene_states::get_scene_state,
        commands::scene_states::upsert_scene_state,
        commands::scene_states::delete_scene_state,
        commands::image_presets::list_image_presets,
        commands::image_presets::create_image_preset,
        commands::image_presets::update_image_preset,
        commands::image_presets::delete_image_preset,
        commands::image_presets::set_default_image_preset,
        commands::conversations::set_conversation_image_preset,
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
        commands::chat::send::send_message,
        commands::chat::attachments::upload_message_attachment,
        commands::chat::attachments::upload_message_attachment_bytes,
        commands::chat::retry::retry_failed_message,
        commands::chat::retry::regenerate_message,
        commands::chat::pipeline::generate_raw,
        commands::chat::pipeline::get_context_stats,
        commands::chat::retry::cancel_generation,
        commands::chat::pipeline::extract_initial_scene,
        commands::npc::list_conversation_npcs,
        commands::npc::promote_npc_to_gallery,
        commands::npc::confirm_npc,
        commands::npc::mark_npc_reviewed,
        commands::npc::refresh_character_profile,
        commands::npc::debug_run_npc_detection,
        commands::npc::generate_npc_portrait,
        commands::npc::approve_npc_portrait,
        commands::npc::reject_npc_portrait,
        commands::npc::get_cast_memory_graph,
        // Personas
        commands::personas::create_persona,
        commands::personas::get_persona,
        commands::personas::list_personas,
        commands::personas::update_persona,
        commands::personas::delete_persona,
        commands::personas::trash_persona,
        commands::personas::restore_persona,
        commands::personas::generate_persona_portrait,
        commands::import::import_persona_card,
        commands::conversations::set_conversation_persona,
        // Trash
        commands::trash::list_trash,
        commands::trash::empty_trash,
        commands::logs::get_backend_logs,
        commands::logs::get_backend_logs_page,
        commands::logs::get_backend_log_path,
    ])
}

/// Recursively copies a directory tree. Fallback for the identifier-rename
/// data migration when `std::fs::rename` fails (e.g. old and new app data
/// dirs land on different volumes, where a rename can't just repoint a
/// directory entry and must actually move bytes).
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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

            // One-time migration: the app identifier changed from com.mythic.app
            // to com.janus.app as part of the Mythic -> Janus rebrand, which
            // moves app_data_dir to a brand-new, empty path on every platform
            // (Roaming/<identifier> on Windows, Application Support/<identifier>
            // on macOS, XDG data dir/<identifier> on Linux). Without this, every
            // conversation/character/lorebook entry a user already has would
            // silently appear to vanish on first launch under the new name —
            // nothing is actually lost, the app would just be looking in the
            // wrong folder. Runs once: only fires when the new dir doesn't
            // exist yet and the old one does; a no-op for fresh installs and
            // for anyone who's already migrated.
            if !app_data_dir.exists() {
                if let Some(data_root) = app_data_dir.parent() {
                    let old_dir = data_root.join("com.mythic.app");
                    if old_dir.exists() {
                        if let Err(e) = std::fs::rename(&old_dir, &app_data_dir) {
                            // Cross-device rename (e.g. old dir on a different
                            // volume) fails with an OS error rather than
                            // silently losing data — fall back to a recursive
                            // copy so the migration still succeeds.
                            eprintln!("Rename migration failed ({e}), falling back to copy");
                            if let Err(copy_err) = copy_dir_recursive(&old_dir, &app_data_dir) {
                                eprintln!("Data migration copy also failed: {copy_err}");
                            }
                        }
                    }
                }
            }

            // Initialize tracing/logging — dual output to stdout (dev console,
            // unchanged from before) AND a persisted file under the app data
            // dir, since a packaged GUI app has no visible console at all and
            // every hidden-bug hunt this session ended up needing whatever got
            // printed to a terminal that happened to still be open. The file
            // is what backs the Settings > Logging tab and its Export button.
            // Must happen before `db::init_database` — that's the first thing
            // that actually logs anything.
            fn build_log_filter() -> EnvFilter {
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("janus_lib=debug,info"))
            }
            let logs_dir = app_data_dir.join("logs");
            std::fs::create_dir_all(&logs_dir).expect("Failed to create logs directory");
            let file_appender = tracing_appender::rolling::RollingFileAppender::new(
                tracing_appender::rolling::Rotation::NEVER,
                &logs_dir,
                "janus.log",
            );
            // `non_blocking` hands back a writer plus a guard that must stay
            // alive for the whole process — its drop flushes and stops the
            // background writer thread. Handing it to Tauri's managed state
            // keeps it alive for exactly the app's lifetime without a leak.
            let (file_writer, log_guard) = tracing_appender::non_blocking(file_appender);
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer().with_filter(build_log_filter()))
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .with_filter(build_log_filter()),
                )
                .init();
            app_handle.manage(log_guard);

            info!("Starting Janus v{}", env!("CARGO_PKG_VERSION"));

            // Initialize the database using Tauri's own async runtime.
            // IMPORTANT: We must NOT create a temporary tokio::runtime::Runtime here.
            // SurrealDB spawns internal async tasks on the current runtime — if that
            // runtime is dropped (as a temporary one would be at end of setup()),
            // those tasks die and the Surreal<Db> handle becomes a dead channel.
            let db =
                tauri::async_runtime::block_on(async { db::init_database(&app_data_dir).await })
                    .expect("Failed to initialize database");

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
                                tracing::warn!(
                                    "Failed to copy seed avatar {:?}: {}",
                                    entry.file_name(),
                                    e
                                );
                            } else {
                                info!("Copied seed avatar: {:?}", entry.file_name());
                            }
                        }
                    }
                }
            }

            // Build a shared HTTP client for all providers
            let http_client = reqwest::Client::builder()
                .user_agent(format!("Janus/{}", env!("CARGO_PKG_VERSION")))
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
                active_generations: Arc::new(AsyncMutex::new(HashMap::new())),
                active_scene_generations: Arc::new(AsyncMutex::new(HashMap::new())),
            };

            app.manage(Arc::new(RwLock::new(state)));

            info!("Janus initialized successfully");
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
            commands::characters::trash_character,
            commands::characters::restore_character,
            commands::characters::upload_character_avatar,
            // Conversations
            commands::conversations::create_conversation,
            commands::conversations::get_conversation,
            commands::conversations::list_conversations,
            commands::conversations::count_conversations,
            commands::conversations::delete_conversation,
            commands::conversations::trash_conversation,
            commands::conversations::restore_conversation,
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
            commands::chat::send::send_message,
            commands::chat::attachments::upload_message_attachment,
            commands::chat::attachments::upload_message_attachment_bytes,
            commands::chat::retry::retry_failed_message,
            commands::chat::retry::regenerate_message,
            commands::chat::pipeline::generate_raw,
            commands::chat::pipeline::get_context_stats,
            commands::chat::retry::cancel_generation,
            commands::chat::pipeline::extract_initial_scene,
            // Import
            commands::import::import_character_card,
            commands::import::get_avatar_path,
            // Scenes
            commands::scenes::generate_scene,
            commands::scenes::generate_video_scene,
            commands::scenes::list_scene_cast_members,
            commands::scenes::cancel_scene_generation,
            commands::scenes::list_scenes,
            commands::scenes::delete_scene,
            commands::scenes::get_scene_path,
            // Lorebook
            commands::lorebook::list_lorebook_entries,
            commands::lorebook::create_lorebook_entry,
            commands::lorebook::toggle_lorebook_entry,
            commands::lorebook::delete_lorebook_entry,
            commands::lorebook::update_lorebook_entry,
            commands::lorebook::import_character_book_entries,
            commands::lorebook::generate_character_lorebook,
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
            commands::character_state::set_message_emotional_snapshot,
            // Scene State
            commands::scene_states::get_scene_state,
            commands::scene_states::upsert_scene_state,
            commands::scene_states::delete_scene_state,
            // Image Presets
            commands::image_presets::list_image_presets,
            commands::image_presets::create_image_preset,
            commands::image_presets::update_image_preset,
            commands::image_presets::delete_image_preset,
            commands::image_presets::set_default_image_preset,
            commands::conversations::set_conversation_image_preset,
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
            // NPCs
            commands::npc::list_conversation_npcs,
            commands::npc::promote_npc_to_gallery,
            commands::npc::confirm_npc,
            commands::npc::mark_npc_reviewed,
            commands::npc::refresh_character_profile,
            commands::npc::debug_run_npc_detection,
            commands::npc::generate_npc_portrait,
            commands::npc::approve_npc_portrait,
            commands::npc::reject_npc_portrait,
            commands::npc::get_cast_memory_graph,
            // Personas
            commands::personas::create_persona,
            commands::personas::get_persona,
            commands::personas::list_personas,
            commands::personas::update_persona,
            commands::personas::delete_persona,
            commands::personas::trash_persona,
            commands::personas::restore_persona,
            commands::personas::generate_persona_portrait,
            commands::import::import_persona_card,
            commands::conversations::set_conversation_persona,
            // Trash
            commands::trash::list_trash,
            commands::trash::empty_trash,
            commands::logs::get_backend_logs,
            commands::logs::get_backend_logs_page,
            commands::logs::get_backend_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Janus");
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
        name: "Janus".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
    }
}
