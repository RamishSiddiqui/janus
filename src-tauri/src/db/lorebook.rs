use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::models::lorebook::LorebookEntry;

pub struct LorebookRepo;

impl LorebookRepo {
    /// Lists lorebook entries for a character (+ global entries where character_id IS NONE).
    pub async fn list(
        db: &Surreal<Db>,
        character_id: &str,
    ) -> Result<Vec<LorebookEntry>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM lorebook_entries
                 WHERE character_id = type::thing('characters', $char_id)
                    OR character_id IS NONE
                 ORDER BY priority DESC, insertion_order ASC",
            )
            .bind(("char_id", character_id.to_string()))
            .await?;

        let entries: Vec<LorebookEntry> = result.take(0)?;
        Ok(entries)
    }

    /// Creates a lorebook entry. `keys` is stored as a native SurrealDB array.
    pub async fn create(
        db: &Surreal<Db>,
        character_id: Option<&str>,
        name: &str,
        keys: Vec<String>,
        content: &str,
        always_active: bool,
    ) -> Result<LorebookEntry, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();

        // Build character_id binding: either a record reference expression or NONE
        let char_binding: serde_json::Value = match character_id {
            Some(cid) => serde_json::json!(cid),
            None => serde_json::Value::Null,
        };

        let query = "CREATE type::thing('lorebook_entries', $id) CONTENT {
            character_id: IF $char_id != NONE THEN type::thing('characters', $char_id) ELSE NONE END,
            name: $name,
            keys: $keys,
            content: $content,
            enabled: true,
            always_active: $always_active,
            priority: 10,
            insertion_order: 100,
        }";

        let mut result = db
            .query(query)
            .bind(("id", id))
            .bind(("char_id", char_binding))
            .bind(("name", name.to_string()))
            .bind(("keys", keys))
            .bind(("content", content.to_string()))
            .bind(("always_active", always_active))
            .await?;

        let created: Option<LorebookEntry> = result.take(0)?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create lorebook entry".into()))
    }

    /// Toggles the enabled state of a lorebook entry.
    pub async fn toggle(
        db: &Surreal<Db>,
        id: &str,
        enabled: bool,
    ) -> Result<(), MythicError> {
        db.query("UPDATE type::thing('lorebook_entries', $id) SET enabled = $enabled")
            .bind(("id", id.to_string()))
            .bind(("enabled", enabled))
            .await?;

        Ok(())
    }

    /// Deletes a lorebook entry by ID.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let _: Option<LorebookEntry> = db.delete(("lorebook_entries", id)).await?;
        Ok(())
    }
}
