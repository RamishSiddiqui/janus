use std::path::Path;
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};
use tracing::info;

use crate::error::MythicError;

pub mod schema;
pub mod seed;
pub mod characters;
pub mod conversations;
pub mod messages;
pub mod memories;
pub mod providers;
pub mod scenes;
pub mod lorebook;
pub mod character_state;
pub mod summaries;

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
    info!("Schema defined successfully, running seed...");
    seed::seed_defaults(&db).await?;

    info!("Debugging conversation list...");
    match crate::db::conversations::ConversationRepo::list(&db, 10, 0).await {
        Ok(convs) => {
            info!("Successfully listed {} conversations", convs.len());
            for c in &convs {
                if c.shared_character_ids.is_some() {
                    info!("  SHARED CONV: {} (id={}) shared_ids={:?}", c.title, c.id, c.shared_character_ids);
                }
            }
        },
        Err(e) => info!("FAILED TO LIST CONVERSATIONS: {:?}", e),
    }
    match crate::db::conversations::ConversationRepo::count(&db).await {
        Ok(count) => info!("Successfully counted conversations: {}", count),
        Err(e) => info!("FAILED TO COUNT CONVERSATIONS: {:?}", e),
    }

    info!("SurrealDB initialized successfully");
    Ok(db)
}
