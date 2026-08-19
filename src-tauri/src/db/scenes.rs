use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use crate::models::scene::Scene;

pub struct SceneRepo;

impl SceneRepo {
    /// Creates a scene record. Returns the persisted Scene.
    pub async fn create(
        db: &Surreal<Db>,
        id: &str,
        conversation_id: &str,
        message_id: Option<&str>,
        media_type: &str,
        prompt: &str,
        file_path: &str,
        caption: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<Scene, MythicError> {
        // Build the message_id binding: either a record reference or None
        let msg_binding: serde_json::Value = match message_id {
            Some(mid) => serde_json::json!(mid),
            None => serde_json::Value::Null,
        };

        let query = "CREATE type::record('scenes', $id) CONTENT {
            conversation_id: type::record('conversations', $conv_id),
            message_id: IF $msg_id != NONE THEN type::record('messages', $msg_id) ELSE NONE END,
            media_type: $media_type,
            prompt: $prompt,
            file_path: $file_path,
            caption: $caption,
            metadata: $metadata,
            created_at: time::now(),
        }";

        let mut result = db
            .query(query)
            .bind(("id", id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .bind((
                "msg_id",
                crate::db::value_bridge::to_surreal_value(msg_binding),
            ))
            .bind(("media_type", media_type.to_string()))
            .bind(("prompt", prompt.to_string()))
            .bind(("file_path", file_path.to_string()))
            .bind(("caption", caption.map(|s| s.to_string())))
            .bind((
                "metadata",
                metadata.map(crate::db::value_bridge::to_surreal_value),
            ))
            .await?;

        let created: Option<Scene> = crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create scene".into()))
    }

    /// Lists scenes for a conversation, ordered by created_at DESC.
    pub async fn list(db: &Surreal<Db>, conversation_id: &str) -> Result<Vec<Scene>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM scenes WHERE conversation_id = type::record('conversations', $conv_id) ORDER BY created_at DESC",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;

        let scenes: Vec<Scene> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(scenes)
    }

    /// Gets a scene's file_path (for deletion cleanup).
    pub async fn get_file_path(db: &Surreal<Db>, id: &str) -> Result<Option<String>, MythicError> {
        let mut result = db
            .query("SELECT file_path FROM type::record('scenes', $id)")
            .bind(("id", id.to_string()))
            .await?;

        // SurrealDB returns an object with `file_path`; extract it
        let row: Option<serde_json::Value> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        Ok(row.and_then(|v| v["file_path"].as_str().map(|s| s.to_string())))
    }

    /// Deletes a scene by ID.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let _: Option<Scene> =
            crate::db::value_bridge::from_value_opt(db.delete(("scenes", id)).await?)?;
        Ok(())
    }
}
