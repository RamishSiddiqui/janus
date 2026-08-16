use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::models::conversation::Message;

pub struct MessageRepo;

impl MessageRepo {
    /// Creates a new message and updates the conversation's active_message_id.
    pub async fn create(
        db: &Surreal<Db>,
        conversation_id: &str,
        role: &str,
        content: &str,
        parent_id: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<Message, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();

        // Build the CREATE query — parent_id and metadata are optional
        let created: Option<Message> = if let Some(pid) = parent_id {
            if let Some(ref meta) = metadata {
                let mut result = db
                    .query("CREATE type::thing('messages', $id) CONTENT {
                        conversation_id: type::thing('conversations', $conv_id),
                        role: $role,
                        content: $content,
                        parent_id: type::thing('messages', $parent_id),
                        metadata: $metadata,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("conv_id", conversation_id.to_string()))
                    .bind(("role", role.to_string()))
                    .bind(("content", content.to_string()))
                    .bind(("parent_id", pid.to_string()))
                    .bind(("metadata", meta.clone()))
                    .await?;
                result.take(0)?
            } else {
                let mut result = db
                    .query("CREATE type::thing('messages', $id) CONTENT {
                        conversation_id: type::thing('conversations', $conv_id),
                        role: $role,
                        content: $content,
                        parent_id: type::thing('messages', $parent_id),
                    }")
                    .bind(("id", id.clone()))
                    .bind(("conv_id", conversation_id.to_string()))
                    .bind(("role", role.to_string()))
                    .bind(("content", content.to_string()))
                    .bind(("parent_id", pid.to_string()))
                    .await?;
                result.take(0)?
            }
        } else {
            if let Some(ref meta) = metadata {
                let mut result = db
                    .query("CREATE type::thing('messages', $id) CONTENT {
                        conversation_id: type::thing('conversations', $conv_id),
                        role: $role,
                        content: $content,
                        metadata: $metadata,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("conv_id", conversation_id.to_string()))
                    .bind(("role", role.to_string()))
                    .bind(("content", content.to_string()))
                    .bind(("metadata", meta.clone()))
                    .await?;
                result.take(0)?
            } else {
                let mut result = db
                    .query("CREATE type::thing('messages', $id) CONTENT {
                        conversation_id: type::thing('conversations', $conv_id),
                        role: $role,
                        content: $content,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("conv_id", conversation_id.to_string()))
                    .bind(("role", role.to_string()))
                    .bind(("content", content.to_string()))
                    .await?;
                result.take(0)?
            }
        };

        // Update the conversation's active_message_id
        db.query("UPDATE type::thing('conversations', $conv_id) SET active_message_id = type::thing('messages', $msg_id), updated_at = time::now()")
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("msg_id", id.clone()))
            .await?;

        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create message".into()))
    }

    /// Gets a single message by ID.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Message, MythicError> {
        let message: Option<Message> = db.select(("messages", id)).await?;
        message.ok_or_else(|| MythicError::NotFound(format!("Message not found: {}", id)))
    }

    /// Updates message content.
    pub async fn update(db: &Surreal<Db>, id: &str, content: &str) -> Result<Message, MythicError> {
        let mut result = db
            .query("UPDATE type::thing('messages', $id) SET content = $content")
            .bind(("id", id.to_string()))
            .bind(("content", content.to_string()))
            .await?;
        let updated: Option<Message> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Message not found: {}", id)))
    }

    /// Stores the chain-of-thought/reasoning trace alongside a message,
    /// separate from `update()` since it's set once at stream completion
    /// rather than edited by the user like `content` is.
    pub async fn set_reasoning(db: &Surreal<Db>, id: &str, reasoning: &str) -> Result<(), MythicError> {
        db.query("UPDATE type::thing('messages', $id) SET reasoning = $reasoning")
            .bind(("id", id.to_string()))
            .bind(("reasoning", reasoning.to_string()))
            .await?;
        Ok(())
    }

    /// Merges `patch`'s top-level keys into a message's `metadata` object,
    /// creating it if the message has none yet. Used for data attached after
    /// the message already exists (e.g. the emotional-state snapshot, which
    /// isn't known until a follow-up LLM call completes post-stream) — unlike
    /// `content`/`reasoning`, this can't be set at creation time.
    pub async fn merge_metadata(
        db: &Surreal<Db>,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<(), MythicError> {
        let existing = Self::get(db, id).await?;
        let mut merged = existing.metadata.unwrap_or_else(|| serde_json::json!({}));
        if let (Some(merged_obj), Some(patch_obj)) = (merged.as_object_mut(), patch.as_object()) {
            for (k, v) in patch_obj {
                merged_obj.insert(k.clone(), v.clone());
            }
        }
        db.query("UPDATE type::thing('messages', $id) SET metadata = $metadata")
            .bind(("id", id.to_string()))
            .bind(("metadata", merged))
            .await?;
        Ok(())
    }

    /// Deletes a message.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let result: Option<Message> = db.delete(("messages", id)).await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!("Message not found: {}", id)));
        }
        Ok(())
    }

    /// Walks the parent_id chain from the given message to root.
    /// Returns messages in chronological order (root -> leaf).
    pub async fn get_branch(db: &Surreal<Db>, message_id: &str) -> Result<Vec<Message>, MythicError> {
        let mut chain = Vec::new();
        let mut current_id = Some(message_id.to_string());

        while let Some(ref id) = current_id {
            let msg: Option<Message> = db.select(("messages", id.as_str())).await?;
            match msg {
                Some(m) => {
                    current_id = m.parent_id.as_ref().map(|t| t.id.to_raw());
                    chain.push(m);
                }
                None => break,
            }
        }

        chain.reverse();
        Ok(chain)
    }

    /// Returns all sibling messages (same parent_id).
    pub async fn get_siblings(db: &Surreal<Db>, message_id: &str) -> Result<Vec<Message>, MythicError> {
        // Get the target message to find its parent_id
        let target = Self::get(db, message_id).await?;

        let siblings: Vec<Message> = if let Some(ref parent_thing) = target.parent_id {
            let parent_raw = parent_thing.id.to_raw();
            let mut result = db
                .query("SELECT * FROM messages WHERE parent_id = type::thing('messages', $parent_id) ORDER BY created_at ASC")
                .bind(("parent_id", parent_raw))
                .await?;
            result.take(0)?
        } else {
            // Root message — find all root messages in the same conversation
            let conv_raw = target.conversation_id.id.to_raw();
            let mut result = db
                .query("SELECT * FROM messages WHERE conversation_id = type::thing('conversations', $conv_id) AND parent_id IS NONE ORDER BY created_at ASC")
                .bind(("conv_id", conv_raw))
                .await?;
            result.take(0)?
        };

        Ok(siblings)
    }

    /// Creates a message attributed to a specific character.
    /// Used for multi-character conversations where each parsed segment
    /// gets its own message with character_id and character_name.
    pub async fn create_with_character(
        db: &Surreal<Db>,
        conversation_id: &str,
        role: &str,
        content: &str,
        parent_id: Option<&str>,
        character_id: &str,
        character_name: &str,
    ) -> Result<Message, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();

        let created: Option<Message> = if let Some(pid) = parent_id {
            let mut result = db
                .query("CREATE type::thing('messages', $id) CONTENT {
                    conversation_id: type::thing('conversations', $conv_id),
                    role: $role,
                    content: $content,
                    parent_id: type::thing('messages', $parent_id),
                    character_id: type::thing('characters', $char_id),
                    character_name: $char_name,
                }")
                .bind(("id", id.clone()))
                .bind(("conv_id", conversation_id.to_string()))
                .bind(("role", role.to_string()))
                .bind(("content", content.to_string()))
                .bind(("parent_id", pid.to_string()))
                .bind(("char_id", character_id.to_string()))
                .bind(("char_name", character_name.to_string()))
                .await?;
            result.take(0)?
        } else {
            let mut result = db
                .query("CREATE type::thing('messages', $id) CONTENT {
                    conversation_id: type::thing('conversations', $conv_id),
                    role: $role,
                    content: $content,
                    character_id: type::thing('characters', $char_id),
                    character_name: $char_name,
                }")
                .bind(("id", id.clone()))
                .bind(("conv_id", conversation_id.to_string()))
                .bind(("role", role.to_string()))
                .bind(("content", content.to_string()))
                .bind(("char_id", character_id.to_string()))
                .bind(("char_name", character_name.to_string()))
                .await?;
            result.take(0)?
        };

        // Update the conversation's active_message_id
        db.query("UPDATE type::thing('conversations', $conv_id) SET active_message_id = type::thing('messages', $msg_id), updated_at = time::now()")
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("msg_id", id.clone()))
            .await?;

        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create character message".into()))
    }
}

