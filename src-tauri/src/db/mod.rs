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

pub async fn init_database(data_dir: &Path) -> Result<Surreal<Db>, MythicError> {
    let db_path = data_dir.join("mythic_surreal");
    info!("Initializing SurrealDB at: {:?}", db_path);

    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let db = Surreal::new::<RocksDb>(&db_path).await?;
    db.use_ns("mythic").use_db("mythic").await?;

    schema::define_schema(&db).await?;
    seed::seed_defaults(&db).await?;

    info!("SurrealDB initialized successfully");
    Ok(db)
}
