use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::MythicError;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::models::memory::{Memory, MemoryGraph, MemoryGraphCharacter, MemoryGraphConversation, MemoryLink};

pub struct MemoryRepo;

impl MemoryRepo {
    /// Lists memories filtered by character_id and/or conversation_id.
    pub async fn list(
        db: &Surreal<Db>,
        character_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<Vec<Memory>, MythicError> {
        let memories: Vec<Memory> = if let Some(char_id) = character_id {
            let mut result = db
                .query("SELECT * FROM memories WHERE character_id = type::thing('characters', $char_id) ORDER BY created_at DESC")
                .bind(("char_id", char_id.to_string()))
                .await?;
            result.take(0)?
        } else if let Some(conv_id) = conversation_id {
            let mut result = db
                .query("SELECT * FROM memories WHERE conversation_id = type::thing('conversations', $conv_id) ORDER BY created_at DESC")
                .bind(("conv_id", conv_id.to_string()))
                .await?;
            result.take(0)?
        } else {
            let mut result = db
                .query("SELECT * FROM memories ORDER BY created_at DESC LIMIT 100")
                .await?;
            result.take(0)?
        };

        Ok(memories)
    }

    /// Lists memories for a conversation, plus this specific character's
    /// canon memories. This ensures canon facts are always available
    /// regardless of memory scope, without leaking an unrelated character's
    /// canon secrets in from elsewhere in the app — the canon clause used to
    /// have no character (or conversation) linkage at all (`OR is_canon =
    /// true`), unioning in literally every canon memory in the database.
    pub async fn list_with_canon(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
    ) -> Result<Vec<Memory>, MythicError> {
        let mut result = db
            .query("SELECT * FROM memories WHERE conversation_id = type::thing('conversations', $conv_id) OR (is_canon = true AND character_id = type::thing('characters', $char_id)) ORDER BY created_at DESC")
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("char_id", character_id.to_string()))
            .await?;
        let memories: Vec<Memory> = result.take(0)?;
        Ok(memories)
    }

    /// Lists memories for a specific character within a conversation,
    /// PLUS that character's canon memories. Used in multi-character prompts
    /// to attribute memories correctly per character.
    pub async fn list_for_character_in_conv(
        db: &Surreal<Db>,
        character_id: &str,
        conversation_id: &str,
    ) -> Result<Vec<Memory>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM memories WHERE \
                    (character_id = type::thing('characters', $char_id) \
                     AND conversation_id = type::thing('conversations', $conv_id)) \
                    OR \
                    (character_id = type::thing('characters', $char_id) \
                     AND is_canon = true) \
                 ORDER BY is_canon DESC, created_at DESC"
            )
            .bind(("char_id", character_id.to_string()))
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let memories: Vec<Memory> = result.take(0)?;
        Ok(memories)
    }

    /// Creates a new memory entry.
    pub async fn create(
        db: &Surreal<Db>,
        character_id: Option<&str>,
        conversation_id: Option<&str>,
        content: &str,
        source: &str,
    ) -> Result<Memory, MythicError> {
        let id = uuid::Uuid::new_v4().to_string();

        let created: Option<Memory> = match (character_id, conversation_id) {
            (Some(char_id), Some(conv_id)) => {
                let mut result = db
                    .query("CREATE type::thing('memories', $id) CONTENT {
                        character_id: type::thing('characters', $char_id),
                        conversation_id: type::thing('conversations', $conv_id),
                        content: $content,
                        source: $source,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("char_id", char_id.to_string()))
                    .bind(("conv_id", conv_id.to_string()))
                    .bind(("content", content.to_string()))
                    .bind(("source", source.to_string()))
                    .await?;
                result.take(0)?
            }
            (Some(char_id), None) => {
                let mut result = db
                    .query("CREATE type::thing('memories', $id) CONTENT {
                        character_id: type::thing('characters', $char_id),
                        content: $content,
                        source: $source,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("char_id", char_id.to_string()))
                    .bind(("content", content.to_string()))
                    .bind(("source", source.to_string()))
                    .await?;
                result.take(0)?
            }
            (None, Some(conv_id)) => {
                let mut result = db
                    .query("CREATE type::thing('memories', $id) CONTENT {
                        conversation_id: type::thing('conversations', $conv_id),
                        content: $content,
                        source: $source,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("conv_id", conv_id.to_string()))
                    .bind(("content", content.to_string()))
                    .bind(("source", source.to_string()))
                    .await?;
                result.take(0)?
            }
            (None, None) => {
                let mut result = db
                    .query("CREATE type::thing('memories', $id) CONTENT {
                        content: $content,
                        source: $source,
                    }")
                    .bind(("id", id.clone()))
                    .bind(("content", content.to_string()))
                    .bind(("source", source.to_string()))
                    .await?;
                result.take(0)?
            }
        };

        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create memory".into()))
    }

    /// Updates a memory's content and increments version.
    pub async fn update(db: &Surreal<Db>, id: &str, content: &str) -> Result<Memory, MythicError> {
        let mut result = db
            .query("UPDATE type::thing('memories', $id) SET content = $content, version = version + 1")
            .bind(("id", id.to_string()))
            .bind(("content", content.to_string()))
            .await?;
        let updated: Option<Memory> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Memory not found: {}", id)))
    }

    /// Bumps a memory's access tracking — call (best-effort, fire-and-forget)
    /// whenever a memory is actually surfaced to the LLM via retrieval, so
    /// frequently-relevant memories accrue a recency signal over time.
    pub async fn bump_access(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        db.query("UPDATE type::thing('memories', $id) SET access_count += 1, last_accessed = time::now()")
            .bind(("id", id.to_string()))
            .await?;
        Ok(())
    }

    /// Sets a memory's importance tier (clamped to 1-10). Used to weight
    /// retrieval ranking independently of semantic relevance.
    pub async fn set_importance(db: &Surreal<Db>, id: &str, importance: i32) -> Result<Memory, MythicError> {
        let clamped = importance.clamp(1, 10);
        let mut result = db
            .query("UPDATE type::thing('memories', $id) SET importance = $importance")
            .bind(("id", id.to_string()))
            .bind(("importance", clamped))
            .await?;
        let updated: Option<Memory> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Memory not found: {}", id)))
    }

    /// Deletes a memory.
    pub async fn delete(db: &Surreal<Db>, id: &str) -> Result<(), MythicError> {
        let result: Option<Memory> = db.delete(("memories", id)).await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!("Memory not found: {}", id)));
        }
        Ok(())
    }

    /// Gets a single memory by ID.
    pub async fn get(db: &Surreal<Db>, id: &str) -> Result<Memory, MythicError> {
        let memory: Option<Memory> = db.select(("memories", id)).await?;
        memory.ok_or_else(|| MythicError::NotFound(format!("Memory not found: {}", id)))
    }

    /// Promotes a memory to canon (character-level).
    pub async fn promote_to_canon(db: &Surreal<Db>, id: &str) -> Result<Memory, MythicError> {
        // Verify the memory exists and has a character_id
        let mem = Self::get(db, id).await?;
        if mem.character_id.is_none() {
            return Err(MythicError::Config(
                "Cannot promote a memory without a character_id to canon".to_string(),
            ));
        }

        let mut result = db
            .query("UPDATE type::thing('memories', $id) SET is_canon = true")
            .bind(("id", id.to_string()))
            .await?;
        let updated: Option<Memory> = result.take(0)?;
        updated.ok_or_else(|| MythicError::NotFound(format!("Memory not found: {}", id)))
    }

    /// Shares a memory to another conversation via RELATE edge.
    /// For 'copy' links: creates a duplicate memory, then RELATE edge.
    /// For 'sync' links: just creates the RELATE edge.
    pub async fn share(
        db: &Surreal<Db>,
        source_memory_id: &str,
        target_conversation_id: &str,
        link_type: &str,
        direction: &str,
        sync_mode: &str,
    ) -> Result<MemoryLink, MythicError> {
        // Fetch source memory to get content/character_id for copy
        let source = Self::get(db, source_memory_id).await?;

        let linked_memory_id: Option<String> = if link_type == "copy" {
            // Create a copy of the memory in the target conversation
            let copy_id = uuid::Uuid::new_v4().to_string();

            if let Some(ref char_thing) = source.character_id {
                let char_id_raw = char_thing.id.to_raw();
                db.query("CREATE type::thing('memories', $copy_id) CONTENT {
                        character_id: type::thing('characters', $char_id),
                        conversation_id: type::thing('conversations', $target_conv_id),
                        content: $content,
                        source: 'auto',
                        parent_id: type::thing('memories', $source_mem_id),
                        version: 1,
                        is_canon: false,
                    }")
                    .bind(("copy_id", copy_id.clone()))
                    .bind(("char_id", char_id_raw))
                    .bind(("target_conv_id", target_conversation_id.to_string()))
                    .bind(("content", source.content.clone()))
                    .bind(("source_mem_id", source_memory_id.to_string()))
                    .await?;
            } else {
                db.query("CREATE type::thing('memories', $copy_id) CONTENT {
                        conversation_id: type::thing('conversations', $target_conv_id),
                        content: $content,
                        source: 'auto',
                        parent_id: type::thing('memories', $source_mem_id),
                        version: 1,
                        is_canon: false,
                    }")
                    .bind(("copy_id", copy_id.clone()))
                    .bind(("target_conv_id", target_conversation_id.to_string()))
                    .bind(("content", source.content.clone()))
                    .bind(("source_mem_id", source_memory_id.to_string()))
                    .await?;
            }

            Some(copy_id)
        } else {
            None
        };

        // Create the RELATE edge
        let src_thing = surrealdb::sql::Thing::from(("memories", source_memory_id));
        let tgt_thing = surrealdb::sql::Thing::from(("conversations", target_conversation_id));

        let link: Option<MemoryLink> = if let Some(ref copy_id) = linked_memory_id {
            let copy_thing = surrealdb::sql::Thing::from(("memories", copy_id.as_str()));
            let mut result = db
                .query("RELATE $src -> memory_link -> $tgt SET
                    link_type = $link_type,
                    direction = $direction,
                    sync_mode = $sync_mode,
                    linked_memory_id = $copy_thing")
                .bind(("src", src_thing))
                .bind(("tgt", tgt_thing))
                .bind(("link_type", link_type.to_string()))
                .bind(("direction", direction.to_string()))
                .bind(("sync_mode", sync_mode.to_string()))
                .bind(("copy_thing", copy_thing))
                .await?;
            result.take(0)?
        } else {
            let mut result = db
                .query("RELATE $src -> memory_link -> $tgt SET
                    link_type = $link_type,
                    direction = $direction,
                    sync_mode = $sync_mode")
                .bind(("src", src_thing))
                .bind(("tgt", tgt_thing))
                .bind(("link_type", link_type.to_string()))
                .bind(("direction", direction.to_string()))
                .bind(("sync_mode", sync_mode.to_string()))
                .await?;
            result.take(0)?
        };

        link.ok_or_else(|| MythicError::DatabaseOp("Failed to create memory link".into()))
    }

    /// Removes a memory link (graph edge).
    pub async fn unlink(db: &Surreal<Db>, link_id: &str) -> Result<(), MythicError> {
        let result: Option<MemoryLink> = db.delete(("memory_link", link_id)).await?;
        if result.is_none() {
            return Err(MythicError::NotFound(format!(
                "Memory link not found: {}",
                link_id
            )));
        }
        Ok(())
    }

    /// Returns the full memory graph for a character.
    ///
    /// Queries memories, links, and conversations separately, then assembles
    /// the graph DTO in Rust code.
    pub async fn get_graph(
        db: &Surreal<Db>,
        character_id: &str,
    ) -> Result<MemoryGraph, MythicError> {
        // 1. Get character name
        #[derive(Debug, serde::Deserialize)]
        struct CharNameRow {
            name: String,
        }

        let mut char_result = db
            .query("SELECT name FROM type::thing('characters', $char_id)")
            .bind(("char_id", character_id.to_string()))
            .await?;
        let char_row: Option<CharNameRow> = char_result.take(0)?;
        let character_name = char_row
            .map(|r| r.name)
            .ok_or_else(|| MythicError::NotFound(format!("Character not found: {}", character_id)))?;

        // 2. All memories for this character
        let mut mem_result = db
            .query("SELECT * FROM memories WHERE character_id = type::thing('characters', $char_id) ORDER BY created_at ASC")
            .bind(("char_id", character_id.to_string()))
            .await?;
        let memories: Vec<Memory> = mem_result.take(0)?;

        // 3. Collect memory IDs for link query
        let memory_ids: Vec<String> = memories.iter().map(|m| m.id.id.to_raw()).collect();

        // 4. All links from these memories
        let links: Vec<MemoryLink> = if memory_ids.is_empty() {
            Vec::new()
        } else {
            // Build Thing values for the IN query
            let memory_things: Vec<surrealdb::sql::Thing> = memory_ids
                .iter()
                .map(|id| surrealdb::sql::Thing::from(("memories", id.as_str())))
                .collect();

            let mut link_result = db
                .query("SELECT * FROM memory_link WHERE in IN $memory_ids")
                .bind(("memory_ids", memory_things))
                .await?;
            link_result.take(0)?
        };

        // 5. All conversations for this character
        #[derive(Debug, serde::Deserialize)]
        struct ConvRow {
            id: surrealdb::sql::Thing,
            title: String,
            #[serde(deserialize_with = "crate::models::deserialize_option_thing")]
            parent_conversation_id: Option<surrealdb::sql::Thing>,
        }

        let mut conv_result = db
            .query("SELECT id, title, parent_conversation_id, updated_at FROM conversations WHERE character_id = type::thing('characters', $char_id) ORDER BY updated_at DESC")
            .bind(("char_id", character_id.to_string()))
            .await?;
        let conv_rows: Vec<ConvRow> = conv_result.take(0)?;

        // 6. Count memories per conversation in Rust
        let mut conv_mem_counts: std::collections::HashMap<String, i32> =
            std::collections::HashMap::new();
        for mem in &memories {
            if let Some(ref conv_thing) = mem.conversation_id {
                let conv_id_raw = conv_thing.id.to_raw();
                *conv_mem_counts.entry(conv_id_raw).or_insert(0) += 1;
            }
        }

        let conversations = conv_rows
            .into_iter()
            .map(|row| {
                let conv_id_raw = row.id.id.to_raw();
                let memory_count = conv_mem_counts.get(&conv_id_raw).copied().unwrap_or(0);
                MemoryGraphConversation {
                    id: conv_id_raw,
                    title: row.title,
                    character_id: character_id.to_string(),
                    memory_count,
                    parent_conversation_id: row
                        .parent_conversation_id
                        .map(|t| t.id.to_raw()),
                }
            })
            .collect();

        Ok(MemoryGraph {
            character_id: character_id.to_string(),
            character_name,
            memories,
            links,
            conversations,
            characters: Vec::new(),
        })
    }

    /// Returns a multi-character "cast graph" scoped to one conversation —
    /// every character in that conversation's cast (gallery mains + NPCs)
    /// plus their combined memories/links. Same 4-query-then-assemble shape
    /// as `get_graph`, but seeded from the conversation's cast instead of a
    /// single character.
    pub async fn get_cast_graph(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<MemoryGraph, MythicError> {
        // 1. Cast members for this conversation
        let cast = ConversationCharacterRepo::list(db, conversation_id).await?;
        let char_things: Vec<surrealdb::sql::Thing> = cast
            .iter()
            .map(|c| c.character_id.clone())
            .collect();
        let characters: Vec<MemoryGraphCharacter> = cast
            .iter()
            .map(|c| MemoryGraphCharacter {
                id: c.character_id.id.to_raw(),
                name: c.character_name.clone(),
            })
            .collect();

        if char_things.is_empty() {
            return Ok(MemoryGraph {
                character_id: String::new(),
                character_name: String::new(),
                memories: Vec::new(),
                links: Vec::new(),
                conversations: Vec::new(),
                characters,
            });
        }

        // 2. Memories for any cast member, scoped to THIS conversation — the
        // previous query filtered by character only, which for a
        // 'character'-scope character (memories shared across all their
        // conversations by design) pulled in that character's entire
        // memory history from every conversation they've ever been in, not
        // just this one's story. That's correct for `build_prompt`'s
        // context injection, but wrong for this visualization, whose whole
        // point is "what happened in this conversation."
        //
        // Canon memories are the one deliberate exception: promoting a
        // memory to canon (see `promote_to_canon`) marks it a permanent,
        // character-defining fact — often seeded outside any single
        // conversation (character creation, a different story entirely) —
        // and the whole point of "canon" is that it's true everywhere the
        // character appears, not just wherever it happened to be recorded.
        // Excluding it here just because its `conversation_id` doesn't
        // match would silently drop a character's core traits/goals from
        // their own cast graph.
        let mut mem_result = db
            .query("SELECT * FROM memories WHERE character_id IN $char_ids AND (conversation_id = type::thing('conversations', $conv_id) OR is_canon = true) ORDER BY created_at ASC")
            .bind(("char_ids", char_things.clone()))
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let memories: Vec<Memory> = mem_result.take(0)?;

        // 3. Collect memory IDs for link query
        let memory_ids: Vec<String> = memories.iter().map(|m| m.id.id.to_raw()).collect();

        // 4. All links from these memories
        let links: Vec<MemoryLink> = if memory_ids.is_empty() {
            Vec::new()
        } else {
            let memory_things: Vec<surrealdb::sql::Thing> = memory_ids
                .iter()
                .map(|id| surrealdb::sql::Thing::from(("memories", id.as_str())))
                .collect();

            let mut link_result = db
                .query("SELECT * FROM memory_link WHERE in IN $memory_ids")
                .bind(("memory_ids", memory_things))
                .await?;
            link_result.take(0)?
        };

        // 5. Just this conversation (kept as a list — and the query still
        // scoped by id rather than hardcoded — so a future branch-aware
        // version can widen it to the conversation's branch family without
        // reshaping anything downstream).
        #[derive(Debug, serde::Deserialize)]
        struct ConvRow {
            id: surrealdb::sql::Thing,
            title: String,
            #[serde(deserialize_with = "crate::models::deserialize_option_thing")]
            parent_conversation_id: Option<surrealdb::sql::Thing>,
        }

        let mut conv_result = db
            .query("SELECT id, title, parent_conversation_id, updated_at FROM conversations WHERE id = type::thing('conversations', $conv_id) ORDER BY updated_at DESC")
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let conv_rows: Vec<ConvRow> = conv_result.take(0)?;

        // 6. Count memories per conversation in Rust
        let mut conv_mem_counts: std::collections::HashMap<String, i32> =
            std::collections::HashMap::new();
        for mem in &memories {
            if let Some(ref conv_thing) = mem.conversation_id {
                let conv_id_raw = conv_thing.id.to_raw();
                *conv_mem_counts.entry(conv_id_raw).or_insert(0) += 1;
            }
        }

        let primary = characters.first();
        let conversations = conv_rows
            .into_iter()
            .map(|row| {
                let conv_id_raw = row.id.id.to_raw();
                let memory_count = conv_mem_counts.get(&conv_id_raw).copied().unwrap_or(0);
                MemoryGraphConversation {
                    id: conv_id_raw,
                    title: row.title,
                    character_id: primary.map(|p| p.id.clone()).unwrap_or_default(),
                    memory_count,
                    parent_conversation_id: row
                        .parent_conversation_id
                        .map(|t| t.id.to_raw()),
                }
            })
            .collect();

        Ok(MemoryGraph {
            character_id: primary.map(|p| p.id.clone()).unwrap_or_default(),
            character_name: primary.map(|p| p.name.clone()).unwrap_or_default(),
            memories,
            links,
            conversations,
            characters,
        })
    }
}
