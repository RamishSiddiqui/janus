use surrealdb::engine::local::Db;
use surrealdb::Surreal;

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
                 WHERE character_id = type::record('characters', $char_id)
                    OR character_id IS NONE
                 ORDER BY priority DESC, insertion_order ASC",
            )
            .bind(("char_id", character_id.to_string()))
            .await?;

        let entries: Vec<LorebookEntry> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
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

        let query = "CREATE type::record('lorebook_entries', $id) CONTENT {
            character_id: IF $char_id != NONE THEN type::record('characters', $char_id) ELSE NONE END,
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
            .bind((
                "char_id",
                crate::db::value_bridge::to_surreal_value(char_binding),
            ))
            .bind(("name", name.to_string()))
            .bind(("keys", keys))
            .bind(("content", content.to_string()))
            .bind(("always_active", always_active))
            .await?;

        let created: Option<LorebookEntry> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create lorebook entry".into()))
    }

    /// Toggles the enabled state of a lorebook entry.
    pub async fn toggle(db: &Surreal<Db>, id: &str, enabled: bool) -> Result<(), MythicError> {
        db.query("UPDATE type::record('lorebook_entries', $id) SET enabled = $enabled")
            .bind(("id", id.to_string()))
            .bind(("enabled", enabled))
            .await?;

        Ok(())
    }

    /// Updates a lorebook entry's editable fields. Unlike `toggle` (which
    /// only flips `enabled`), this covers everything the entry has —
    /// previously there was no way to change an entry's name/keys/content/
    /// always_active/priority/insertion_order after creation at all, only
    /// toggle-on-off or delete-and-recreate.
    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        db: &Surreal<Db>,
        id: &str,
        name: &str,
        keys: Vec<String>,
        content: &str,
        always_active: bool,
        priority: i32,
        insertion_order: i32,
    ) -> Result<LorebookEntry, MythicError> {
        let mut result = db
            .query(
                "UPDATE type::record('lorebook_entries', $id) SET \
                    name = $name, keys = $keys, content = $content, \
                    always_active = $always_active, priority = $priority, \
                    insertion_order = $insertion_order",
            )
            .bind(("id", id.to_string()))
            .bind(("name", name.to_string()))
            .bind(("keys", keys))
            .bind(("content", content.to_string()))
            .bind(("always_active", always_active))
            .bind(("priority", priority))
            .bind(("insertion_order", insertion_order))
            .await?;

        let updated: Option<LorebookEntry> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Lorebook entry not found: {}", id)))
    }

    /// Imports every entry from a parsed Character Card V2 `character_book`
    /// as real, persisted lorebook entries for `character_id`. Used both by
    /// PNG import (so a card's built-in lorebook actually participates in
    /// chat generation, not just a read-only display) and by a manual
    /// "Import from Character Card" action for characters that were
    /// imported before this existed.
    ///
    /// `constant` (the V2 spec's field name) maps to this app's
    /// `always_active`. `case_sensitive` has no equivalent here yet — every
    /// entry's keywords match case-insensitively, same as manually-created
    /// entries.
    pub async fn import_from_character_book(
        db: &Surreal<Db>,
        character_id: &str,
        book: &crate::models::character::CharacterBook,
    ) -> Result<Vec<LorebookEntry>, MythicError> {
        let mut imported = Vec::with_capacity(book.entries.len());
        for (i, entry) in book.entries.iter().enumerate() {
            if entry.keys.is_empty() && !entry.constant {
                // Neither keyword-triggered nor always-active — would never
                // fire, so importing it would just be silent dead weight.
                continue;
            }
            let name = entry
                .name
                .clone()
                .unwrap_or_else(|| format!("Entry {}", i + 1));
            let created = Self::create(
                db,
                Some(character_id),
                &name,
                entry.keys.clone(),
                &entry.content,
                entry.constant,
            )
            .await?;
            // `create` always sets enabled=true/priority=10/insertion_order=100 —
            // carry over the source card's real values in a follow-up update
            // when they differ, rather than losing that fidelity.
            let needs_update = entry.priority != 10 || entry.insertion_order != 100;
            let mut final_entry = if needs_update {
                Self::update(
                    db,
                    &crate::db::value_bridge::record_id_to_string(&created.id),
                    &name,
                    entry.keys.clone(),
                    &entry.content,
                    entry.constant,
                    entry.priority,
                    entry.insertion_order,
                )
                .await?
            } else {
                created
            };
            if !entry.enabled {
                Self::toggle(
                    db,
                    &crate::db::value_bridge::record_id_to_string(&final_entry.id),
                    false,
                )
                .await?;
                final_entry.enabled = false;
            }
            imported.push(final_entry);
        }
        Ok(imported)
    }

    /// Deletes a lorebook entry by ID.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let _: Option<LorebookEntry> =
            crate::db::value_bridge::from_value_opt(db.delete(("lorebook_entries", id)).await?)?;
        Ok(())
    }
}
