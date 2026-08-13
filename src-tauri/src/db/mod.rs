use std::path::Path;
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};
use tracing::info;

use crate::error::MythicError;

pub mod migrations;
pub mod schema;
pub mod seed;
pub mod characters;
pub mod conversations;
pub mod conversation_characters;
pub mod messages;
pub mod memories;
pub mod providers;
pub mod scenes;
pub mod scene_states;
pub mod image_presets;
pub mod ai_horde_models;
pub mod lorebook;
pub mod character_state;
pub mod summaries;
pub mod embeddings;
pub mod npc_candidates;
pub mod personas;

pub async fn init_database(data_dir: &Path) -> Result<Surreal<Db>, MythicError> {
    let db_path = data_dir.join("mythic_surreal");
    info!("Initializing SurrealDB at: {:?}", db_path);

    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let db = Surreal::new::<RocksDb>(db_path).await?;
    db.use_ns("mythic").use_db("mythic").await?;

    info!("Running schema definitions...");
    schema::define_schema(&db).await?;
    info!("Schema defined, running migrations...");
    migrations::run_pending(&db).await?;
    info!("Migrations complete, running seed...");
    seed::seed_defaults(&db).await?;

    info!("Janus data store initialized successfully");
    Ok(db)
}
