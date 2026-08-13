use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::models::persona::Persona;

pub struct PersonaRepo;

impl PersonaRepo {
    /// Creates a new persona. Returns the created persona.
    pub async fn create(
        db: &Surreal<Db>,
        name: &str,
        data: serde_json::Value,
    ) -> Result<Persona, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();
        let created: Option<Persona> = db
            .create(("personas", &*id))
            .content(serde_json::json!({
                "name": name,
                "spec": "chara_card_v2",
                "data": data,
            }))
            .await?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create persona".into()))
    }

    /// Gets a single persona by ID.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Persona, MythicError> {
        let persona: Option<Persona> = db.select(("personas", id)).await?;
        persona.ok_or_else(|| MythicError::NotFound(format!("Persona not found: {}", id)))
    }

    /// Lists all non-trashed personas ordered by updated_at DESC.
    pub async fn list(db: &Surreal<Db>) -> Result<Vec<Persona>, MythicError> {
        let mut result = db
            .query("SELECT * FROM personas WHERE deleted_at IS NONE ORDER BY updated_at DESC")
            .await?;
        let personas: Vec<Persona> = result.take(0)?;
        Ok(personas)
    }

    /// Updates a persona. Only non-None fields are updated.
    pub async fn update(
        db: &Surreal<Db>,
        id: &str,
        name: Option<&str>,
        data: Option<serde_json::Value>,
        avatar_path: Option<&str>,
    ) -> Result<Persona, MythicError> {
        let mut sets = Vec::new();
        let mut bindings_json = serde_json::Map::new();

        if let Some(name) = name {
            sets.push("name = $name");
            bindings_json.insert("name".into(), serde_json::Value::String(name.to_string()));
        }
        if let Some(data) = data {
            sets.push("data = $data");
            bindings_json.insert("data".into(), data);
        }
        if let Some(avatar_path) = avatar_path {
            sets.push("avatar_path = $avatar_path");
            bindings_json.insert(
                "avatar_path".into(),
                serde_json::Value::String(avatar_path.to_string()),
            );
        }

        if sets.is_empty() {
            return Self::get(db, id).await;
        }

        sets.push("updated_at = time::now()");
        let query = format!("UPDATE type::thing('personas', $id) SET {}", sets.join(", "));
        bindings_json.insert("id".into(), serde_json::Value::String(id.to_string()));
        let mut result = db
            .query(&query)
            .bind(serde_json::Value::Object(bindings_json))
            .await?;

        let updated: Option<Persona> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Persona not found: {}", id)))
    }

    /// Sets (or clears) a persona's avatar path — used by portrait
    /// generation and PNG import.
    pub async fn set_avatar(
        db: &Surreal<Db>,
        id: &str,
        avatar_path: Option<&str>,
    ) -> Result<Persona, MythicError> {
        let mut result = db
            .query("UPDATE type::thing('personas', $id) SET avatar_path = $avatar_path, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .bind(("avatar_path", avatar_path.map(|s| s.to_string())))
            .await?;
        let updated: Option<Persona> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Persona not found: {}", id)))
    }

    /// Permanently deletes a persona by ID. Cascade (clearing
    /// `conversations.persona_id`) is handled by the `cascade_persona_delete`
    /// SurrealDB event. Only ever called from the Trash view — normal
    /// "Delete" should call `trash` instead.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        // See `retry_on_conflict` in error.rs.
        let result: Option<Persona> =
            crate::error::retry_on_conflict(|| async { db.delete(("personas", id)).await }).await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!("Persona not found: {}", id)));
        }
        Ok(())
    }

    /// Soft-deletes a persona — moves it to Trash. See the matching comment
    /// on `ConversationRepo::trash`.
    pub async fn trash(db: &Surreal<Db>, id: &str) -> Result<Persona, MythicError> {
        let mut result = db
            .query("UPDATE type::thing('personas', $id) SET deleted_at = time::now()")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Persona> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Persona not found: {}", id)))
    }

    /// Restores a trashed persona.
    pub async fn restore(db: &Surreal<Db>, id: &str) -> Result<Persona, MythicError> {
        let mut result = db
            .query("UPDATE type::thing('personas', $id) SET deleted_at = NONE")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Persona> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Persona not found: {}", id)))
    }

    /// Lists trashed personas, most recently trashed first.
    pub async fn list_trashed(db: &Surreal<Db>) -> Result<Vec<Persona>, MythicError> {
        let mut result = db
            .query("SELECT * FROM personas WHERE deleted_at IS NOT NONE ORDER BY deleted_at DESC")
            .await?;
        let personas: Vec<Persona> = result.take(0)?;
        Ok(personas)
    }
}
