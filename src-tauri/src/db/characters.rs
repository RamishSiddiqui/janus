use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::models::character::Character;

pub struct CharacterRepo;

impl CharacterRepo {
    /// Creates a new character. Returns the created character.
    pub async fn create(
        db: &Surreal<Db>,
        name: &str,
        data: serde_json::Value,
    ) -> Result<Character, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();
        let created: Option<Character> = db
            .create(("characters", &*id))
            .content(serde_json::json!({
                "name": name,
                "spec": "chara_card_v2",
                "data": data,
            }))
            .await?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create character".into()))
    }

    /// Gets a single character by ID.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> {
        let character: Option<Character> = db.select(("characters", id)).await?;
        character.ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Lists all characters ordered by updated_at DESC.
    pub async fn list(db: &Surreal<Db>) -> Result<Vec<Character>, MythicError> {
        let mut result = db
            .query("SELECT * FROM characters ORDER BY updated_at DESC")
            .await?;
        let characters: Vec<Character> = result.take(0)?;
        Ok(characters)
    }

    /// Updates a character. Only non-None fields are updated.
    pub async fn update(
        db: &Surreal<Db>,
        id: &str,
        name: Option<&str>,
        data: Option<serde_json::Value>,
        avatar_path: Option<&str>,
    ) -> Result<Character, MythicError> {
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
        let query = format!("UPDATE type::thing('characters', $id) SET {}", sets.join(", "));
        bindings_json.insert("id".into(), serde_json::Value::String(id.to_string()));
        let mut result = db
            .query(&query)
            .bind(serde_json::Value::Object(bindings_json))
            .await?;

        let updated: Option<Character> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Deletes a character by ID. Cascade is handled by SurrealDB events.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let result: Option<Character> = db.delete(("characters", id)).await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!(
                "Character not found: {}",
                id
            )));
        }
        Ok(())
    }
}
