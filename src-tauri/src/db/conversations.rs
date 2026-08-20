use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::info;

use crate::error::MythicError;
use crate::models::conversation::{Conversation, Message, MessageRole, SearchResult};

pub struct ConversationRepo;

impl ConversationRepo {
    /// Creates a new conversation, optionally linked to a character and/or a
    /// persona (the user's own stand-in for this conversation).
    pub async fn create(
        db: &Surreal<Db>,
        character_id: Option<&str>,
        title: Option<&str>,
        persona_id: Option<&str>,
    ) -> Result<Conversation, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();
        let title = title.unwrap_or("New Chat");

        let mut fields = vec!["title: $title".to_string()];
        if character_id.is_some() {
            fields.push("character_id: type::record('characters', $char_id)".to_string());
        }
        if persona_id.is_some() {
            fields.push("persona_id: type::record('personas', $persona_id)".to_string());
        }
        let query_str = format!(
            "CREATE type::record('conversations', $id) CONTENT {{ {} }}",
            fields.join(", ")
        );

        let mut q = db
            .query(query_str)
            .bind(("id", id.clone()))
            .bind(("title", title.to_string()));
        if let Some(char_id) = character_id {
            q = q.bind(("char_id", char_id.to_string()));
        }
        if let Some(pid) = persona_id {
            q = q.bind(("persona_id", pid.to_string()));
        }
        let mut result = q.await?;
        let conv: Option<Conversation> = crate::db::value_bridge::from_value_opt(result.take(0)?)?;

        conv.ok_or_else(|| MythicError::DatabaseOp("Failed to create conversation".into()))
    }

    /// Gets a single (non-trashed) conversation by ID. Trash/restore operate
    /// on the row directly via UPDATE and never call this, so excluding
    /// trashed conversations here doesn't affect them — everything else
    /// (chat generation, the get_conversation command, the NPC pipeline)
    /// should treat a trashed conversation as gone, the same as list()/
    /// count() already do, not silently keep it fully usable.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Conversation, MythicError> {
        let conversation: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(db.select(("conversations", id)).await?)?;
        let conversation = conversation
            .ok_or_else(|| MythicError::NotFound(format!("Conversation not found: {}", id)))?;
        if conversation.deleted_at.is_some() {
            return Err(MythicError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }
        Ok(conversation)
    }

    /// Lists conversations with pagination, ordered by most recently updated.
    /// Excludes trashed conversations — see `list_trashed` for those.
    pub async fn list(
        db: &Surreal<Db>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Conversation>, MythicError> {
        let mut result = db
            .query("SELECT * FROM conversations WHERE deleted_at IS NONE ORDER BY updated_at DESC LIMIT $limit START $offset")
            .bind(("limit", limit))
            .bind(("offset", offset))
            .await?;
        let conversations: Vec<Conversation> =
            crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(conversations)
    }

    /// Returns the total number of non-trashed conversations (for pagination).
    pub async fn count(db: &Surreal<Db>) -> Result<u32, MythicError> {
        let mut result = db
            .query("SELECT count() FROM conversations WHERE deleted_at IS NONE GROUP ALL")
            .await?;
        // SurrealDB returns [{ count: N }] from GROUP ALL
        let count_row: Option<serde_json::Value> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        let count = count_row
            .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
            .unwrap_or(0);
        Ok(count as u32)
    }

    /// Permanently deletes a conversation by ID. Cascade is handled by
    /// SurrealDB events. Only ever called from the Trash view — normal
    /// "Delete" from the chat list should call `trash` instead.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        // Cascade event touches many tables (messages, memories, scenes,
        // cast rows, ...) — retried on a transaction conflict rather than
        // failing silently when the user deletes several conversations in
        // quick succession. See `retry_on_conflict`.
        let result: Option<Conversation> = crate::db::value_bridge::from_value_opt(
            crate::error::retry_on_conflict(|| async { db.delete(("conversations", id)).await })
                .await?,
        )?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }
        Ok(())
    }

    /// Soft-deletes a conversation — moves it to Trash. The row (and every
    /// cascade-linked row) stays fully intact; only `deleted_at` changes, so
    /// this is instant and durable the moment it returns, unlike the old
    /// client-side "undo window" that lost the delete entirely if the app
    /// reloaded before the timer fired.
    pub async fn trash(db: &Surreal<Db>, id: &str) -> Result<Conversation, MythicError> {
        let mut result = db
            .query("UPDATE type::record('conversations', $id) SET deleted_at = time::now()")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Conversation not found: {}", id)))
    }

    /// Restores a trashed conversation.
    pub async fn restore(db: &Surreal<Db>, id: &str) -> Result<Conversation, MythicError> {
        let mut result = db
            .query("UPDATE type::record('conversations', $id) SET deleted_at = NONE")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Conversation not found: {}", id)))
    }

    /// Lists trashed conversations, most recently trashed first.
    pub async fn list_trashed(db: &Surreal<Db>) -> Result<Vec<Conversation>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM conversations WHERE deleted_at IS NOT NONE ORDER BY deleted_at DESC",
            )
            .await?;
        let conversations: Vec<Conversation> =
            crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(conversations)
    }

    /// Retrieves all messages in a conversation, ordered chronologically.
    ///
    /// Combines two strategies to ensure completeness:
    /// 1. Standard query by conversation_id (gets most messages)
    /// 2. Branch walk from active_message_id (catches any messages that
    ///    might have a mismatched conversation_id due to SurrealDB Thing quirks)
    pub async fn get_messages(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Vec<Message>, MythicError> {
        // Strategy 1: standard conversation_id query
        let mut result = db
            .query("SELECT * FROM messages WHERE conversation_id = type::record('conversations', $conv_id) ORDER BY created_at ASC")
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let mut messages: Vec<Message> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;

        // Strategy 2: walk the branch from active_message_id to catch missing messages
        let conv = Self::get(db, conversation_id).await?;
        if let Some(ref active_msg_thing) = conv.active_message_id {
            let active_id = crate::db::value_bridge::record_id_to_string(active_msg_thing);
            let known_ids: std::collections::HashSet<String> = messages
                .iter()
                .map(|m| crate::db::value_bridge::record_id_to_string(&m.id))
                .collect();

            // Walk backward from active_message_id following parent_id chain
            let mut current_id = Some(active_id);
            let mut missing: Vec<Message> = Vec::new();
            let mut visited = std::collections::HashSet::new();

            while let Some(ref id) = current_id {
                if visited.contains(id) {
                    break;
                }
                visited.insert(id.clone());

                if !known_ids.contains(id) {
                    // This message is in the branch but wasn't returned by conv_id query
                    let msg: Option<Message> = crate::db::value_bridge::from_value_opt(
                        db.select(("messages", id.as_str())).await?,
                    )?;
                    if let Some(m) = msg {
                        current_id = m
                            .parent_id
                            .as_ref()
                            .map(crate::db::value_bridge::record_id_to_string);
                        info!(
                            "[get_messages] Recovered missing message {} (role={:?}, char={:?}) via branch walk",
                            crate::db::value_bridge::record_id_to_string(&m.id), m.role, m.character_name
                        );
                        missing.push(m);
                    } else {
                        break;
                    }
                } else {
                    // Message exists in the query results — follow its parent
                    if let Some(existing) = messages
                        .iter()
                        .find(|m| crate::db::value_bridge::record_id_to_string(&m.id) == *id)
                    {
                        current_id = existing
                            .parent_id
                            .as_ref()
                            .map(crate::db::value_bridge::record_id_to_string);
                    } else {
                        break;
                    }
                }
            }

            if !missing.is_empty() {
                info!(
                    "[get_messages] Recovered {} missing messages for conversation {}",
                    missing.len(),
                    conversation_id
                );
                messages.extend(missing);
                // Re-sort by created_at to maintain chronological order
                messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            }
        }

        Ok(messages)
    }

    /// Sets the active message pointer for branch navigation.
    pub async fn set_active_message(
        db: &Surreal<Db>,
        conversation_id: &str,
        message_id: &str,
    ) -> Result<(), MythicError> {
        let mut result = db
            .query("UPDATE type::record('conversations', $conv_id) SET active_message_id = type::record('messages', $msg_id), updated_at = time::now()")
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("msg_id", message_id.to_string()))
            .await?;
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        if updated.is_none() {
            return Err(MythicError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }
        Ok(())
    }

    /// Sets (or clears, if `preset_id` is `None`) this conversation's chosen
    /// image-generation preset.
    pub async fn set_image_preset(
        db: &Surreal<Db>,
        conversation_id: &str,
        preset_id: Option<&str>,
    ) -> Result<(), MythicError> {
        let mut result = match preset_id {
            Some(pid) => db
                .query("UPDATE type::record('conversations', $conv_id) SET image_preset_id = type::record('image_presets', $preset_id), updated_at = time::now()")
                .bind(("conv_id", conversation_id.to_string()))
                .bind(("preset_id", pid.to_string()))
                .await?,
            None => db
                .query("UPDATE type::record('conversations', $conv_id) SET image_preset_id = NONE, updated_at = time::now()")
                .bind(("conv_id", conversation_id.to_string()))
                .await?,
        };
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        if updated.is_none() {
            return Err(MythicError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }
        Ok(())
    }

    /// Sets (or clears, if `persona_id` is `None`) this conversation's
    /// chosen persona.
    pub async fn set_persona(
        db: &Surreal<Db>,
        conversation_id: &str,
        persona_id: Option<&str>,
    ) -> Result<(), MythicError> {
        let mut result = match persona_id {
            Some(pid) => db
                .query("UPDATE type::record('conversations', $conv_id) SET persona_id = type::record('personas', $persona_id), updated_at = time::now()")
                .bind(("conv_id", conversation_id.to_string()))
                .bind(("persona_id", pid.to_string()))
                .await?,
            None => db
                .query("UPDATE type::record('conversations', $conv_id) SET persona_id = NONE, updated_at = time::now()")
                .bind(("conv_id", conversation_id.to_string()))
                .await?,
        };
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        if updated.is_none() {
            return Err(MythicError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }
        Ok(())
    }

    /// Updates a conversation's title.
    pub async fn update_title(
        db: &Surreal<Db>,
        id: &str,
        title: &str,
    ) -> Result<Conversation, MythicError> {
        let mut result = db
            .query("UPDATE type::record('conversations', $id) SET title = $title, updated_at = time::now()")
            .bind(("id", id.to_string()))
            .bind(("title", title.to_string()))
            .await?;
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Conversation not found: {}", id)))
    }

    /// Sets the memory scope for a conversation.
    pub async fn set_memory_scope(
        db: &Surreal<Db>,
        conversation_id: &str,
        scope: &str,
    ) -> Result<(), MythicError> {
        let mut result = db
            .query("UPDATE type::record('conversations', $conv_id) SET memory_scope = $scope, updated_at = time::now()")
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("scope", scope.to_string()))
            .await?;
        let updated: Option<Conversation> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        if updated.is_none() {
            return Err(MythicError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }
        Ok(())
    }

    /// Creates a branch (fork) of an existing conversation at a given message.
    ///
    /// Walks the parent chain from `branch_point_msg_id` back to root, copies
    /// all messages with fresh IDs (remapping parent references), then copies
    /// all memories with `RELATE` graph edges for link tracking.
    pub async fn branch(
        db: &Surreal<Db>,
        parent_id: &str,
        branch_point_msg_id: &str,
        title: Option<&str>,
    ) -> Result<Conversation, MythicError> {
        // 1. Fetch parent conversation
        let parent = Self::get(db, parent_id).await?;

        // 2. Fetch all messages in parent conversation
        #[derive(Debug, Clone, serde::Deserialize)]
        struct MsgRow {
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            id: surrealdb::types::RecordId,
            role: String,
            content: String,
            #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
            parent_id: Option<surrealdb::types::RecordId>,
        }

        let mut msg_result = db
            .query("SELECT id, role, content, parent_id FROM messages WHERE conversation_id = type::record('conversations', $conv_id)")
            .bind(("conv_id", parent_id.to_string()))
            .await?;
        let all_msgs: Vec<MsgRow> = crate::db::value_bridge::from_value_vec(msg_result.take(0)?)?;

        // Build lookup by raw ID string
        let by_id: std::collections::HashMap<String, &MsgRow> = all_msgs
            .iter()
            .map(|m| (crate::db::value_bridge::record_id_to_string(&m.id), m))
            .collect();

        // Walk backward from branch point to root
        let mut path_ids: Vec<String> = Vec::new();
        let mut current: Option<String> = Some(branch_point_msg_id.to_string());
        let mut visited = std::collections::HashSet::new();
        while let Some(id) = current {
            if !by_id.contains_key(&id) || visited.contains(&id) {
                break;
            }
            visited.insert(id.clone());
            path_ids.push(id.clone());
            current = by_id[&id]
                .parent_id
                .as_ref()
                .map(crate::db::value_bridge::record_id_to_string);
        }
        path_ids.reverse(); // now root → branch_point

        // 3. Create the new conversation
        let new_conv_id = uuid::Uuid::new_v4().to_string();
        let title = title.unwrap_or(&parent.title);

        // Extract character_id raw string if present
        let char_id_str = parent
            .character_id
            .as_ref()
            .map(crate::db::value_bridge::record_id_to_string);

        if let Some(ref char_id) = char_id_str {
            db.query(
                "CREATE type::record('conversations', $id) CONTENT {
                    title: $title,
                    character_id: type::record('characters', $char_id),
                    memory_scope: $scope,
                    parent_conversation_id: type::record('conversations', $parent_id),
                    branch_point_message_id: type::record('messages', $branch_msg_id),
                }",
            )
            .bind(("id", new_conv_id.clone()))
            .bind(("title", title.to_string()))
            .bind(("char_id", char_id.clone()))
            .bind(("scope", parent.memory_scope.clone()))
            .bind(("parent_id", parent_id.to_string()))
            .bind(("branch_msg_id", branch_point_msg_id.to_string()))
            .await?;
        } else {
            db.query(
                "CREATE type::record('conversations', $id) CONTENT {
                    title: $title,
                    memory_scope: $scope,
                    parent_conversation_id: type::record('conversations', $parent_id),
                    branch_point_message_id: type::record('messages', $branch_msg_id),
                }",
            )
            .bind(("id", new_conv_id.clone()))
            .bind(("title", title.to_string()))
            .bind(("scope", parent.memory_scope.clone()))
            .bind(("parent_id", parent_id.to_string()))
            .bind(("branch_msg_id", branch_point_msg_id.to_string()))
            .await?;
        }

        // 4. Copy messages with fresh IDs, remapping parent_id references
        let mut old_to_new: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut last_new_id = String::new();

        for old_id in &path_ids {
            let msg = &by_id[old_id];
            let new_msg_id = uuid::Uuid::new_v4().to_string();
            let new_parent_id = msg
                .parent_id
                .as_ref()
                .and_then(|pid| old_to_new.get(&crate::db::value_bridge::record_id_to_string(pid)))
                .cloned();

            if let Some(ref new_pid) = new_parent_id {
                db.query(
                    "CREATE type::record('messages', $id) CONTENT {
                        conversation_id: type::record('conversations', $conv_id),
                        role: $role,
                        content: $content,
                        parent_id: type::record('messages', $parent_id),
                    }",
                )
                .bind(("id", new_msg_id.clone()))
                .bind(("conv_id", new_conv_id.clone()))
                .bind(("role", msg.role.clone()))
                .bind(("content", msg.content.clone()))
                .bind(("parent_id", new_pid.clone()))
                .await?;
            } else {
                db.query(
                    "CREATE type::record('messages', $id) CONTENT {
                        conversation_id: type::record('conversations', $conv_id),
                        role: $role,
                        content: $content,
                    }",
                )
                .bind(("id", new_msg_id.clone()))
                .bind(("conv_id", new_conv_id.clone()))
                .bind(("role", msg.role.clone()))
                .bind(("content", msg.content.clone()))
                .await?;
            }

            old_to_new.insert(old_id.clone(), new_msg_id.clone());
            last_new_id = new_msg_id;
        }

        // 5. Set active_message_id to the last copied message
        if !last_new_id.is_empty() {
            db.query("UPDATE type::record('conversations', $conv_id) SET active_message_id = type::record('messages', $msg_id), updated_at = time::now()")
                .bind(("conv_id", new_conv_id.clone()))
                .bind(("msg_id", last_new_id))
                .await?;
        }

        // 6. Copy memories from parent conversation → new conversation
        #[derive(Debug, Clone, serde::Deserialize)]
        struct MemRow {
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            id: surrealdb::types::RecordId,
            #[serde(default, deserialize_with = "crate::models::deserialize_option_thing")]
            character_id: Option<surrealdb::types::RecordId>,
            content: String,
        }

        let mut mem_result = db
            .query("SELECT id, character_id, content FROM memories WHERE conversation_id = type::record('conversations', $conv_id)")
            .bind(("conv_id", parent_id.to_string()))
            .await?;
        let parent_mems: Vec<MemRow> =
            crate::db::value_bridge::from_value_vec(mem_result.take(0)?)?;

        for mem in &parent_mems {
            let copy_id = uuid::Uuid::new_v4().to_string();
            let source_mem_id = crate::db::value_bridge::record_id_to_string(&mem.id);

            // Create a copy of the memory in the new conversation
            if let Some(ref char_thing) = mem.character_id {
                let char_id_raw = crate::db::value_bridge::record_id_to_string(char_thing);
                db.query(
                    "CREATE type::record('memories', $id) CONTENT {
                        character_id: type::record('characters', $char_id),
                        conversation_id: type::record('conversations', $conv_id),
                        content: $content,
                        source: 'auto',
                        parent_id: type::record('memories', $parent_mem_id),
                        version: 1,
                        is_canon: false,
                    }",
                )
                .bind(("id", copy_id.clone()))
                .bind(("char_id", char_id_raw))
                .bind(("conv_id", new_conv_id.clone()))
                .bind(("content", mem.content.clone()))
                .bind(("parent_mem_id", source_mem_id.clone()))
                .await?;
            } else {
                db.query(
                    "CREATE type::record('memories', $id) CONTENT {
                        conversation_id: type::record('conversations', $conv_id),
                        content: $content,
                        source: 'auto',
                        parent_id: type::record('memories', $parent_mem_id),
                        version: 1,
                        is_canon: false,
                    }",
                )
                .bind(("id", copy_id.clone()))
                .bind(("conv_id", new_conv_id.clone()))
                .bind(("content", mem.content.clone()))
                .bind(("parent_mem_id", source_mem_id.clone()))
                .await?;
            }

            // Create the memory_link graph edge via RELATE
            db.query("RELATE type::record('memories', $source_mem_id) -> memory_link -> type::record('conversations', $conv_id) SET
                    link_type = 'copy',
                    direction = 'one_way',
                    sync_mode = 'auto',
                    linked_memory_id = type::record('memories', $copy_id)")
                .bind(("source_mem_id", source_mem_id))
                .bind(("conv_id", new_conv_id.clone()))
                .bind(("copy_id", copy_id))
                .await?;
        }

        info!(
            "Branched conversation {} → {} ({} messages, {} memories copied)",
            parent_id,
            new_conv_id,
            path_ids.len(),
            parent_mems.len()
        );

        Self::get(db, &new_conv_id).await
    }

    /// Searches message content using SurrealDB full-text search.
    ///
    /// Returns results with highlighted snippets, conversation titles,
    /// and character names for display in the search overlay.
    pub async fn search_messages(
        db: &Surreal<Db>,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SearchResult>, MythicError> {
        let mut result = db
            .query(
                "SELECT
                    id AS message_id,
                    conversation_id,
                    role,
                    content,
                    search::highlight('<mark>', '</mark>', 4) AS snippet,
                    conversation_id.title AS conversation_title,
                    conversation_id.character_id.name AS character_name,
                    created_at,
                    search::score(4) AS relevance
                FROM messages
                WHERE content @4@ $query
                    AND conversation_id.deleted_at IS NONE
                ORDER BY relevance DESC
                LIMIT $limit",
            )
            .bind(("query", query.to_string()))
            .bind(("limit", limit))
            .await?;

        #[derive(Debug, serde::Deserialize)]
        struct SearchRow {
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            message_id: surrealdb::types::RecordId,
            #[serde(deserialize_with = "crate::models::deserialize_thing")]
            conversation_id: surrealdb::types::RecordId,
            role: String,
            content: String,
            snippet: Option<String>,
            conversation_title: Option<String>,
            character_name: Option<String>,
            created_at: String,
        }

        let rows: Vec<SearchRow> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let role = match row.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    _ => MessageRole::User,
                };
                SearchResult {
                    message_id: crate::db::value_bridge::record_id_to_string(&row.message_id),
                    conversation_id: crate::db::value_bridge::record_id_to_string(
                        &row.conversation_id,
                    ),
                    role,
                    content: row.content,
                    snippet: row.snippet.unwrap_or_default(),
                    conversation_title: row.conversation_title.unwrap_or_default(),
                    character_name: row.character_name,
                    created_at: row.created_at,
                }
            })
            .collect())
    }
}
