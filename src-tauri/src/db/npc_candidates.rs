use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::error::MythicError;
use crate::models::npc_candidate::{NpcCandidate, NpcDetectionState};

pub struct NpcCandidateRepo;

impl NpcCandidateRepo {
    /// Ensures a cadence-tracking row exists for this conversation, then
    /// returns it. Uses the conversation id itself as the deterministic
    /// record id (one row per conversation, mirrors `character_states`'s
    /// unique-index-per-key convention but as a single-part key here since
    /// there's only one dimension to key on).
    async fn get_or_init_state(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<NpcDetectionState, MythicError> {
        let mut result = db
            .query(
                "UPSERT type::record('npc_detection_state', $conv_id) \
                 MERGE { conversation_id: type::record('conversations', $conv_id) }",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let state: Option<NpcDetectionState> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        state.ok_or_else(|| MythicError::DatabaseOp("Failed to init npc_detection_state".into()))
    }

    /// Advances this conversation's cadence counter for `message_id` and
    /// returns whether a detector pass is due now.
    ///
    /// Guards against the forced (notable-event) and periodic-cadence call
    /// sites both firing for the exact same message: if `message_id` was
    /// already the last one scanned, this is a no-op returning `false`.
    /// Otherwise the counter increments (or resets to 0 if due), and a pass
    /// is due when `forced` is true or the counter has reached `cadence`.
    pub async fn bump_and_check_due(
        db: &Surreal<Db>,
        conversation_id: &str,
        message_id: &str,
        forced: bool,
        cadence: i32,
    ) -> Result<bool, MythicError> {
        let state = Self::get_or_init_state(db, conversation_id).await?;

        if state.last_scanned_message_id.as_deref() == Some(message_id) {
            return Ok(false);
        }

        let messages_since_scan = state.messages_since_scan + 1;
        let due = forced || messages_since_scan >= cadence;
        let next_count = if due { 0 } else { messages_since_scan };

        db.query(
            "UPDATE type::record('npc_detection_state', $conv_id) SET \
                messages_since_scan = $count, \
                last_scanned_message_id = $message_id, \
                updated_at = time::now()",
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("count", next_count))
        .bind(("message_id", message_id.to_string()))
        .await?
        .check()
        .map_err(|e| MythicError::DatabaseOp(format!("npc_detection_state update: {}", e)))?;

        Ok(due)
    }

    /// Upserts a detected candidate name for this conversation. Only
    /// `recurring`/`pivotal`-tagged mentions increment `pass_count` — the
    /// detector is instructed to never return `background` tags at all, but
    /// this guard is kept defensively in case one slips through.
    pub async fn upsert_candidate(
        db: &Surreal<Db>,
        conversation_id: &str,
        name: &str,
        tag: &str,
    ) -> Result<NpcCandidate, MythicError> {
        let candidate_key = name.trim().to_lowercase();
        let increments = matches!(tag, "recurring" | "pivotal");

        let mut existing_result = db
            .query(
                "SELECT * FROM npc_candidates \
                 WHERE conversation_id = type::record('conversations', $conv_id) AND candidate_key = $key",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("key", candidate_key.clone()))
            .await?;
        let existing: Option<NpcCandidate> =
            crate::db::value_bridge::from_value_opt(existing_result.take(0)?)?;

        if let Some(existing) = existing {
            let new_count = if increments {
                existing.pass_count + 1
            } else {
                existing.pass_count
            };
            let mut result = db
                .query(
                    "UPDATE type::record('npc_candidates', $id) SET pass_count = $count, tag = $tag, last_seen_at = time::now()",
                )
                .bind(("id", crate::db::value_bridge::record_id_to_string(&existing.id)))
                .bind(("count", new_count))
                .bind(("tag", tag.to_string()))
                .await?;
            let updated: Option<NpcCandidate> =
                crate::db::value_bridge::from_value_opt(result.take(0)?)?;
            return updated
                .ok_or_else(|| MythicError::DatabaseOp("Failed to update npc_candidate".into()));
        }

        let mut result = db
            .query(
                "CREATE npc_candidates CONTENT { \
                    conversation_id: type::record('conversations', $conv_id), \
                    candidate_key: $key, \
                    display_name: $name, \
                    tag: $tag, \
                    pass_count: $count, \
                    status: 'pending', \
                }",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .bind(("key", candidate_key))
            .bind(("name", name.to_string()))
            .bind(("tag", tag.to_string()))
            .bind(("count", if increments { 1 } else { 0 }))
            .await?;
        let created: Option<NpcCandidate> =
            crate::db::value_bridge::from_value_opt(result.take(0)?)?;
        created.ok_or_else(|| MythicError::DatabaseOp("Failed to create npc_candidate".into()))
    }

    /// All candidate display names ever seen for this conversation
    /// (regardless of status) — feeds the detector's "never rediscover a
    /// name" exclusion list, alongside the conversation's actual cast names.
    pub async fn list_known_names(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Vec<String>, MythicError> {
        #[derive(serde::Deserialize)]
        struct NameRow {
            display_name: String,
        }
        let mut result = db
            .query("SELECT display_name FROM npc_candidates WHERE conversation_id = type::record('conversations', $conv_id)")
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let rows: Vec<NameRow> = crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(rows.into_iter().map(|r| r.display_name).collect())
    }

    /// Candidates that have crossed the two-pass debounce threshold and
    /// haven't had a profile generated for them yet.
    pub async fn get_debounced(
        db: &Surreal<Db>,
        conversation_id: &str,
    ) -> Result<Vec<NpcCandidate>, MythicError> {
        let mut result = db
            .query(
                "SELECT * FROM npc_candidates \
                 WHERE conversation_id = type::record('conversations', $conv_id) \
                 AND pass_count >= 2 AND status = 'pending'",
            )
            .bind(("conv_id", conversation_id.to_string()))
            .await?;
        let candidates: Vec<NpcCandidate> =
            crate::db::value_bridge::from_value_vec(result.take(0)?)?;
        Ok(candidates)
    }

    /// Marks a candidate as resolved into a real character row, so it's
    /// never re-generated.
    pub async fn mark_created(
        db: &Surreal<Db>,
        candidate_id: &str,
        character_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "UPDATE type::record('npc_candidates', $id) SET \
                status = 'created', \
                resulting_character_id = type::record('characters', $char_id)",
        )
        .bind(("id", candidate_id.to_string()))
        .bind(("char_id", character_id.to_string()))
        .await?
        .check()
        .map_err(|e| MythicError::DatabaseOp(format!("npc_candidate mark_created: {}", e)))?;
        Ok(())
    }

    /// Best-effort counterpart to [`mark_created`] for the manual "Confirm"
    /// action — resolves whatever pending candidate row (if any) points at
    /// `character_id` in this conversation, so a later periodic detection
    /// pass doesn't redundantly re-run Stage 2 profile generation for a
    /// character the user already confirmed by hand. A no-op (not an error)
    /// if no matching row exists, since a manually-confirmed character may
    /// never have been auto-detected as a candidate at all (e.g. one added
    /// straight from the Gallery to the cast).
    pub async fn mark_created_by_character(
        db: &Surreal<Db>,
        conversation_id: &str,
        character_id: &str,
    ) -> Result<(), MythicError> {
        db.query(
            "UPDATE npc_candidates SET status = 'created' WHERE \
                conversation_id = type::record('conversations', $conv_id) AND \
                resulting_character_id = type::record('characters', $char_id) AND \
                status = 'pending'",
        )
        .bind(("conv_id", conversation_id.to_string()))
        .bind(("char_id", character_id.to_string()))
        .await?
        .check()
        .map_err(|e| {
            MythicError::DatabaseOp(format!("npc_candidate mark_created_by_character: {}", e))
        })?;
        Ok(())
    }

    /// Links a just-registered placeholder character to its candidate row,
    /// so once the two-pass debounce is crossed, Stage 2 fills in that same
    /// character's real profile instead of creating a second one. Does NOT
    /// touch `status` — that still only flips to 'created' once Stage 2
    /// actually runs.
    pub async fn set_placeholder_character(
        db: &Surreal<Db>,
        candidate_id: &str,
        character_id: &str,
    ) -> Result<(), MythicError> {
        db.query("UPDATE type::record('npc_candidates', $id) SET resulting_character_id = type::record('characters', $char_id)")
            .bind(("id", candidate_id.to_string()))
            .bind(("char_id", character_id.to_string()))
            .await?
            .check()
            .map_err(|e| MythicError::DatabaseOp(format!("npc_candidate set_placeholder_character: {}", e)))?;
        Ok(())
    }
}
