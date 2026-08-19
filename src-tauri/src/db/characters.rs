use surrealdb::engine::local::Db;
use surrealdb::types::Value;
use surrealdb::Surreal;

use crate::db::value_bridge::{from_value_opt, from_value_vec, to_surreal_value};
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
        let created: Option<Value> = db
            .create(("characters", &*id))
            .content(to_surreal_value(serde_json::json!({
                "name": name,
                "spec": "chara_card_v2",
                "data": data,
            })))
            .await?;
        from_value_opt(created)?
            .ok_or_else(|| MythicError::DatabaseOp("Failed to create character".into()))
    }

    /// Gets a single character by ID.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> {
        let character: Option<Value> = db.select(("characters", id)).await?;
        from_value_opt(character)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Lists all gallery characters ordered by updated_at DESC. Auto-generated
    /// NPCs (`origin = 'npc'`) are excluded — they only live in the
    /// conversation that spawned them until the user promotes them.
    ///
    /// `origin = NONE` is treated as gallery too: the schema's `DEFAULT
    /// 'gallery'` only applies when a row is written without the field, not
    /// retroactively to rows that existed before this field was added — so
    /// every character created before this migration has no stored `origin`
    /// at all and must be matched explicitly here.
    pub async fn list(db: &Surreal<Db>) -> Result<Vec<Character>, MythicError> {
        let mut result = db
            .query("SELECT * FROM characters WHERE (origin = 'gallery' OR origin = NONE) AND deleted_at IS NONE ORDER BY updated_at DESC")
            .await?;
        let raw: Vec<Value> = result.take(0)?;
        from_value_vec(raw)
    }

    /// Creates an auto-generated NPC character (`origin = 'npc'`,
    /// `profile_reviewed = false`) — used by the NPC-detection pipeline, never
    /// by user-facing character creation.
    pub async fn create_npc(
        db: &Surreal<Db>,
        name: &str,
        data: serde_json::Value,
    ) -> Result<Character, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();
        let created: Option<Value> = db
            .create(("characters", &*id))
            .content(to_surreal_value(serde_json::json!({
                "name": name,
                "spec": "chara_card_v2",
                "data": data,
                "origin": "npc",
                "profile_reviewed": false,
            })))
            .await?;
        from_value_opt(created)?
            .ok_or_else(|| MythicError::DatabaseOp("Failed to create NPC character".into()))
    }

    /// Sets a character's `origin` — used to promote an NPC (`'npc'` →
    /// `'gallery'`) into a real standalone Gallery character.
    pub async fn set_origin(
        db: &Surreal<Db>,
        id: &str,
        origin: &str,
    ) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET origin = $origin, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .bind(("origin", origin.to_string()))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Marks an NPC's auto-generated profile as reviewed — clears the
    /// needs-attention indicator for the "new profile" half of its condition.
    pub async fn mark_reviewed(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET profile_reviewed = true, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Clears `profile_reviewed` — used after a story-driven profile refresh
    /// (auto or manual) so the change surfaces through the same
    /// needs-attention indicator a freshly-generated NPC profile does,
    /// rather than silently rewriting the card with nothing to signal it.
    pub async fn flag_needs_review(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET profile_reviewed = false, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Fills in an NPC's real generated profile over the lightweight
    /// placeholder that was created the moment it was first detected (so it
    /// could speak/respond right away). Re-flags `profile_reviewed = false`
    /// — the placeholder itself doesn't need review, but a freshly-written
    /// backstory does.
    pub async fn update_npc_profile(
        db: &Surreal<Db>,
        id: &str,
        name: &str,
        data: serde_json::Value,
    ) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET name = $name, data = $data, profile_reviewed = false, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .bind(("name", name.to_string()))
            .bind(("data", to_surreal_value(data)))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Sets a character's avatar path and portrait review status — used by
    /// NPC portrait generation/approval/rejection.
    pub async fn set_portrait(
        db: &Surreal<Db>,
        id: &str,
        avatar_path: Option<&str>,
        status: &str,
    ) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET avatar_path = $avatar_path, portrait_status = $status, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .bind(("avatar_path", avatar_path.map(|s| s.to_string())))
            .bind(("status", status.to_string()))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
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
        let query = format!(
            "UPDATE type::record('characters', $id) SET {}",
            sets.join(", ")
        );
        bindings_json.insert("id".into(), serde_json::Value::String(id.to_string()));
        let mut result = db
            .query(&query)
            .bind(to_surreal_value(serde_json::Value::Object(bindings_json)))
            .await?;

        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Permanently deletes a character by ID. Cascade is handled by SurrealDB
    /// events. Only ever called from the Trash view — normal "Delete" from
    /// Gallery should call `trash` instead.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        // See `retry_on_conflict` — the cascade event here touches
        // conversations/memories/lorebook_entries, susceptible to the same
        // transaction-conflict failure as ConversationRepo::delete.
        let result: Option<Value> =
            crate::error::retry_on_conflict(|| async { db.delete(("characters", id)).await })
                .await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!(
                "Character not found: {}",
                id
            )));
        }
        Ok(())
    }

    /// Soft-deletes a character — moves it to Trash. See the matching
    /// comment on `ConversationRepo::trash`.
    pub async fn trash(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET deleted_at = time::now()")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Restores a trashed character.
    pub async fn restore(db: &Surreal<Db>, id: &str) -> Result<Character, MythicError> {
        let mut result = db
            .query("UPDATE type::record('characters', $id) SET deleted_at = NONE")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Value> = result.take(0)?;
        from_value_opt(updated)?
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", id)))
    }

    /// Lists trashed characters, most recently trashed first.
    pub async fn list_trashed(db: &Surreal<Db>) -> Result<Vec<Character>, MythicError> {
        let mut result = db
            .query("SELECT * FROM characters WHERE deleted_at IS NOT NONE ORDER BY deleted_at DESC")
            .await?;
        let raw: Vec<Value> = result.take(0)?;
        from_value_vec(raw)
    }
}
