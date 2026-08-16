//! Commands for the auto-generated NPC cast feature (Phase A: manual/dev
//! trigger + read/promote/review; Phase B wires automatic detection into
//! live chat; Phase C adds portraits; Phase D adds the per-conversation
//! cast memory graph).

use std::sync::Arc;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tauri::{Manager, State};
use tokio::sync::RwLock;
use tracing::info;

use crate::commands::scenes::generate_via_generic_provider;
use crate::providers::ai_horde::generate_via_ai_horde;
use crate::context::npc::pipeline::run_npc_detection;
use crate::context::npc::profile_generator;
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider, resolve_model_id};
use crate::db::characters::CharacterRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::image_presets::ImagePresetRepo;
use crate::db::memories::MemoryRepo;
use crate::db::npc_candidates::NpcCandidateRepo;
use crate::db::providers::ProviderRepo;
use crate::error::{truncate_at_char_boundary, MythicError};
use crate::models::character::Character;
use crate::models::memory::MemoryGraph;
use crate::models::provider::{ImageGenParams, ProviderAdapter};
use crate::providers::unified::RigProvider;
use crate::AppState;

/// Lists the auto-detected cast currently in a conversation — both
/// `role = 'transient'` (just spoke for the first time, not yet confirmed
/// significant) and `role = 'npc'` (crossed the two-pass debounce and got a
/// real profile) members, plus already-promoted-to-gallery ones, since
/// promotion only changes `origin`, not cast membership. Transients are
/// included (not just 'npc') so a character who's clearly speaking and
/// developing a real identity doesn't just vanish from this panel while
/// waiting on the background debounce — the frontend badges them as
/// "Unconfirmed" rather than hiding them outright.
#[tauri::command]
#[specta::specta]
pub async fn list_conversation_npcs(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<Vec<Character>, MythicError> {
    let state = state.read().await;
    let cast = ConversationCharacterRepo::list(&state.db, &conversation_id).await?;
    let npc_ids: Vec<String> = cast
        .iter()
        .filter(|c| c.role == "npc" || c.role == "transient")
        .map(|c| c.character_id.id.to_raw())
        .collect();

    let mut npcs = Vec::with_capacity(npc_ids.len());
    for id in npc_ids {
        if let Ok(character) = CharacterRepo::get(&state.db, &id).await {
            npcs.push(character);
        }
    }
    Ok(npcs)
}

/// Promotes an auto-generated NPC into a real, standalone Gallery character
/// (`origin: 'npc' -> 'gallery'`). No cast/memory data moves — the NPC's
/// memories were already keyed by its own real `character_id` from creation,
/// and it keeps its existing `conversation_characters` row (still shows in
/// this conversation's cast) while now also being independently listable
/// and startable as a solo chat from the Gallery.
#[tauri::command]
#[specta::specta]
pub async fn promote_npc_to_gallery(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Character, MythicError> {
    let state = state.read().await;
    let character = CharacterRepo::set_origin(&state.db, &character_id, "gallery").await?;
    info!("Promoted NPC to gallery: {} ({})", character.name, character_id);
    Ok(character)
}

/// Manually promotes an auto-detected NPC from `role: 'transient'`
/// ("Unconfirmed" in the UI) straight to `role: 'npc'`, skipping the
/// automatic two-pass detector debounce (`pipeline::run_npc_detection`).
/// That debounce is entirely invisible to the user — a character can speak
/// repeatedly and be directly addressed without the periodic detection
/// pass ever happening to re-flag them as recurring, leaving them stuck as
/// "Unconfirmed" indefinitely with no visible reason and, until now, no way
/// to override it.
#[tauri::command]
#[specta::specta]
pub async fn confirm_npc(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    character_id: String,
) -> Result<(), MythicError> {
    let state = state.read().await;
    ConversationCharacterRepo::set_role(&state.db, &conversation_id, &character_id, "npc").await?;
    // Best-effort: resolve any still-pending candidate row too, so a later
    // periodic detection pass doesn't redundantly regenerate her profile if
    // pass_count happens to independently reach the threshold anyway.
    if let Err(e) = NpcCandidateRepo::mark_created_by_character(&state.db, &conversation_id, &character_id).await {
        tracing::debug!("[confirm_npc] Failed to resolve candidate row for {}: {}", character_id, e);
    }
    info!("Manually confirmed NPC {} in conversation {}", character_id, conversation_id);
    Ok(())
}

/// Marks an NPC's auto-generated profile as reviewed — clears the "new
/// profile" half of the needs-attention indicator's condition.
#[tauri::command]
#[specta::specta]
pub async fn mark_npc_reviewed(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Character, MythicError> {
    let state = state.read().await;
    CharacterRepo::mark_reviewed(&state.db, &character_id).await
}

/// Outcome of [`refresh_character_profile`] — the frontend uses `scope` to
/// tell the user which thing actually happened, since a shared character's
/// card is deliberately left untouched (see the matching comment below).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ProfileRefreshResult {
    pub character: Character,
    /// "character" — the card's description/personality/scenario were
    /// updated directly (this character only appears in this conversation).
    /// "memory" — this character is shared across multiple conversations,
    /// so the refresh was saved as a conversation-scoped memory instead of
    /// touching the shared card, to avoid one story's details bleeding into
    /// another conversation's version of the same character. `character` in
    /// this case is returned unchanged.
    pub scope: String,
}

/// Finds every conversation this character is linked to — as a cast member
/// (`conversation_characters`) or as a conversation's own primary character
/// (`conversations.character_id`). Used to decide whether a profile refresh
/// is safe to write to the shared character card (exclusive to one
/// conversation) or must be scoped to a memory instead (shared).
async fn linked_conversation_ids(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    character_id: &str,
) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();

    #[derive(serde::Deserialize)]
    struct ConvIdRow {
        conversation_id: surrealdb::sql::Thing,
    }
    if let Ok(mut result) = db
        .query("SELECT conversation_id FROM conversation_characters WHERE character_id = type::thing('characters', $char_id)")
        .bind(("char_id", character_id.to_string()))
        .await
    {
        if let Ok(rows) = result.take::<Vec<ConvIdRow>>(0) {
            ids.extend(rows.into_iter().map(|r| r.conversation_id.id.to_raw()));
        }
    }

    #[derive(serde::Deserialize)]
    struct ConvPrimaryRow {
        id: surrealdb::sql::Thing,
    }
    if let Ok(mut result) = db
        .query("SELECT id FROM conversations WHERE character_id = type::thing('characters', $char_id)")
        .bind(("char_id", character_id.to_string()))
        .await
    {
        if let Ok(rows) = result.take::<Vec<ConvPrimaryRow>>(0) {
            ids.extend(rows.into_iter().map(|r| r.id.id.to_raw()));
        }
    }

    ids
}

/// Refreshes a character's description/personality/scenario against how
/// they've actually appeared in this conversation — canon facts (settled,
/// character-level truth) plus recent dialogue (voice/tone anchor), with
/// the character's own current profile passed in so the model refines
/// rather than reinvents (and knows to disregard the "just spoke for the
/// first time" placeholder as if it were empty). Used by both the manual
/// "Refresh from Story" button and the automatic still-placeholder trigger
/// in the NPC detection pipeline.
///
/// If this character appears in more than just this one conversation, the
/// card is deliberately left untouched — see `ProfileRefreshResult::scope`.
#[tauri::command]
#[specta::specta]
pub async fn refresh_character_profile(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
    conversation_id: String,
    // User-editable override from Settings > Prompts > Character Profile
    // Refresh. `None` (e.g. the automatic pipeline trigger, which has no
    // frontend settings to read) falls back to the built-in default.
    system_prompt: Option<String>,
) -> Result<ProfileRefreshResult, MythicError> {
    let state_guard = state.read().await;
    let db = state_guard.db.clone();
    drop(state_guard);

    let provider_config = get_default_llm_provider(&db).await?;
    let provider = create_rig_provider(&provider_config)?;
    let model_id = resolve_model_id(None, &provider_config, &db).await?;

    perform_profile_refresh(
        &db, &character_id, &conversation_id, None, &provider, &model_id, system_prompt.as_deref(),
    )
    .await
}

/// The actual refresh logic behind [`refresh_character_profile`], factored
/// out so the automatic still-placeholder trigger in `pipeline.rs` can reuse
/// it without going through a Tauri command (it already has `db`/provider/
/// model in scope from the detection pass it's running inside).
///
/// `recent_dialogue_override`: when `Some`, used verbatim as the story
/// context instead of querying the last 20 messages — the automatic trigger
/// passes the response it just generated the detection pass from, so it
/// doesn't need a second DB round trip for text it already has in hand.
/// Pulls a large recent batch of messages and filters down to the ones that
/// actually mention or were spoken by `character_name`, falling back to the
/// plain recent window if that search turns up nothing (e.g. they're
/// referred to only by pronoun/title, never by name). Shared by profile
/// refresh and lorebook generation — both need the same "how has this
/// character actually shown up in the story" context.
pub async fn gather_character_dialogue(
    db: &Surreal<Db>,
    conversation_id: &str,
    character_name: &str,
) -> String {
    #[derive(serde::Deserialize)]
    struct MsgRow {
        role: String,
        content: String,
        character_name: Option<String>,
    }
    let mut dialogue = String::new();
    let name_lower = character_name.to_lowercase();
    // `created_at` must be in the SELECT list for ORDER BY on it to parse at
    // all (this SurrealDB version rejects ordering by a field the
    // projection doesn't include).
    let query_result = db
        .query(
            "SELECT role, content, character_name, created_at FROM messages \
             WHERE conversation_id = type::thing('conversations', $conv_id) \
             ORDER BY created_at DESC LIMIT 200"
        )
        .bind(("conv_id", conversation_id.to_string()))
        .await;
    if let Err(ref e) = query_result {
        tracing::warn!("[gather_character_dialogue] dialogue query failed for conversation {}: {}", conversation_id, e);
    }
    if let Ok(mut result) = query_result {
        let take_result = result.take::<Vec<MsgRow>>(0);
        if let Err(ref e) = take_result {
            tracing::warn!("[gather_character_dialogue] dialogue row deserialize failed for conversation {}: {}", conversation_id, e);
        }
        if let Ok(all_rows) = take_result {
            info!("[gather_character_dialogue] dialogue query returned {} row(s) for conversation {}", all_rows.len(), conversation_id);
            let mut relevant: Vec<&MsgRow> = all_rows
                .iter()
                .filter(|r| {
                    r.content.to_lowercase().contains(&name_lower)
                        || r.character_name.as_deref().map(|n| n.eq_ignore_ascii_case(character_name)).unwrap_or(false)
                })
                .take(20)
                .collect();
            if relevant.is_empty() {
                relevant = all_rows.iter().take(20).collect();
            }
            let mut rows: Vec<&MsgRow> = relevant;
            rows.reverse();
            for row in rows {
                let speaker = row.character_name.clone().unwrap_or_else(|| row.role.clone());
                dialogue.push_str(&format!("{}: {}\n\n", speaker, row.content));
            }
        }
    }
    dialogue
}

pub async fn perform_profile_refresh(
    db: &Surreal<Db>,
    character_id: &str,
    conversation_id: &str,
    recent_dialogue_override: Option<&str>,
    provider: &RigProvider,
    model_id: &str,
    system_prompt_override: Option<&str>,
) -> Result<ProfileRefreshResult, MythicError> {
    let character = CharacterRepo::get(db, character_id).await?;

    // Already exactly the right scope — this conversation's own memories for
    // this character, plus their canon memories from anywhere (see the
    // repo doc-comment). Previously this was further filtered down to only
    // `is_canon` entries, which silently dropped every regular auto-
    // extracted conversation memory (is_canon defaults to false) — exactly
    // where a detail like "assigned a stillbirth" would actually live,
    // leaving refreshes with almost nothing to work with.
    let known_facts: Vec<String> = MemoryRepo::list_for_character_in_conv(db, character_id, conversation_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.content)
        .collect();

    let recent_dialogue = match recent_dialogue_override {
        Some(text) => text.to_string(),
        None => gather_character_dialogue(db, conversation_id, &character.name).await,
    };

    info!(
        "[profile_refresh] '{}' ({}): {} known fact(s), {} chars of dialogue context. Facts: {:?}. Dialogue preview: {:?}",
        character.name, character_id, known_facts.len(), recent_dialogue.len(), known_facts,
        truncate_at_char_boundary(&recent_dialogue, 300),
    );

    let cast = ConversationCharacterRepo::list(db, conversation_id).await.unwrap_or_default();
    let mut existing_cast: Vec<(String, String)> = Vec::new();
    if let Ok(conv) = ConversationRepo::get(db, conversation_id).await {
        if let Some(pc_id) = conv.character_id {
            if pc_id.id.to_raw() != character_id {
                if let Ok(pc) = CharacterRepo::get(db, &pc_id.id.to_raw()).await {
                    let desc = pc.data.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    existing_cast.push((pc.name, desc));
                }
            }
        }
    }
    for member in &cast {
        let member_id = member.character_id.id.to_raw();
        if member_id == character_id {
            continue;
        }
        if let Ok(c) = CharacterRepo::get(db, &member_id).await {
            let desc = c.data.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            existing_cast.push((c.name, desc));
        }
    }

    let current_description = character.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let current_personality = character.data.get("personality").and_then(|v| v.as_str()).unwrap_or("");
    let current_scenario = character.data.get("scenario").and_then(|v| v.as_str()).unwrap_or("");

    let refined = profile_generator::refine_profile(
        provider, model_id, &character.name,
        current_description, current_personality, current_scenario,
        &known_facts, &recent_dialogue, &existing_cast,
        system_prompt_override,
    ).await?;

    let linked = linked_conversation_ids(db, character_id).await;
    let is_shared = linked.iter().any(|id| id != conversation_id);

    if is_shared {
        // Don't touch the shared card — save the refined understanding as a
        // conversation-scoped memory instead, same as any other auto-
        // extracted memory, so it only informs this story, not every other
        // conversation this character happens to also be in.
        let content = format!(
            "In this story: {} {}",
            refined.description, refined.personality
        ).trim().to_string();
        let _ = MemoryRepo::create(db, Some(character_id), Some(conversation_id), &content, "auto").await;
        info!("Character '{}' is shared across conversations — saved profile refresh as a memory instead of editing the card", character.name);
        return Ok(ProfileRefreshResult { character, scope: "memory".to_string() });
    }

    let mut merged_data = character.data.clone();
    if let Some(obj) = merged_data.as_object_mut() {
        obj.insert("description".to_string(), serde_json::Value::String(refined.description));
        obj.insert("personality".to_string(), serde_json::Value::String(refined.personality));
        obj.insert("scenario".to_string(), serde_json::Value::String(refined.scenario));
    }

    CharacterRepo::update(db, character_id, None, Some(merged_data), None).await?;
    let updated = CharacterRepo::flag_needs_review(db, character_id).await?;
    info!("Refreshed profile for character '{}' ({}) from conversation {}", updated.name, character_id, conversation_id);

    Ok(ProfileRefreshResult { character: updated, scope: "character".to_string() })
}

/// Generates a portrait for an NPC via the configured image provider,
/// framed from its auto-generated description. Silently skips — returns the
/// character unchanged, no error — if no image provider is configured; an
/// auto-approved placeholder gradient standing in for a face would be worse
/// than no portrait at all.
#[tauri::command]
#[specta::specta]
pub async fn generate_npc_portrait(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
    conversation_id: String,
    auto_approve: bool,
) -> Result<Character, MythicError> {
    let state_guard = state.read().await;
    let character = CharacterRepo::get(&state_guard.db, &character_id).await?;

    let provider = ProviderRepo::get_default(&state_guard.db, "image").await?;
    let Some(provider) = provider else {
        return Ok(character);
    };

    let description = character.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let truncated_desc = truncate_at_char_boundary(description, 400);
    // A concealed identity means the story is deliberately withholding this
    // character's face — a normal clear portrait would reveal/fabricate the
    // exact thing being withheld. Frame the prompt around concealment
    // instead of a clear headshot.
    let identity_concealed = character.data.get("identity_concealed").and_then(|v| v.as_bool()).unwrap_or(false);
    let prompt = if identity_concealed {
        format!(
            "a mysterious hooded or shadowed figure, face obscured or turned away, concealed identity, {}, atmospheric, no visible face",
            truncated_desc,
        )
    } else {
        format!(
            "portrait of {}, {}, character portrait, detailed face, upper body",
            character.name, truncated_desc,
        )
    };
    let params = ImageGenParams {
        prompt,
        width: 512,
        height: 512,
        ..Default::default()
    };

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MythicError::Config(format!("Failed to resolve app data dir: {}", e)))?;
    let portraits_dir = app_data_dir.join("portraits");
    tokio::fs::create_dir_all(&portraits_dir).await?;
    let filename = format!("{}.png", character_id);
    let file_path = portraits_dir.join(&filename);
    let relative_path = format!("portraits/{}", filename);

    let image_bytes = if provider.adapter == ProviderAdapter::AiHorde {
        let preset = ImagePresetRepo::resolve_for_conversation(&state_guard.db, &conversation_id).await?;
        // Namespaced key so this never collides with the conversation's own
        // scene-generation progress UI / single-flight lock — both are keyed
        // by whatever string is passed as `conversation_id` here.
        let portrait_key = format!("npc-portrait-{}", character_id);
        let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut active = state_guard.active_scene_generations.lock().await;
            if active.contains_key(&portrait_key) {
                return Err(MythicError::Provider(
                    "A portrait generation is already in progress for this character".to_string(),
                ));
            }
            active.insert(portrait_key.clone(), cancel_flag.clone());
        }

        let result = generate_via_ai_horde(
            &app, &portrait_key, &state_guard.http_client, &provider, &params,
            preset.as_ref(), None, None, None, &cancel_flag,
        ).await;

        state_guard.active_scene_generations.lock().await.remove(&portrait_key);
        result?.0
    } else {
        generate_via_generic_provider(&state_guard.http_client, &provider, &params).await?.0
    };

    tokio::fs::write(&file_path, &image_bytes).await?;

    // A premature/wrong reveal is worse than an ordinary bad scene image —
    // concealed-identity portraits always require manual review, regardless
    // of the auto-approve setting.
    let status = if identity_concealed || !auto_approve { "pending_review" } else { "approved" };
    let updated = CharacterRepo::set_portrait(&state_guard.db, &character_id, Some(&relative_path), status).await?;
    info!("Generated NPC portrait for {} (status={})", updated.name, status);
    Ok(updated)
}

/// Approves a pending NPC portrait (`portrait_status -> 'approved'`) — the
/// avatar image itself is left unchanged.
#[tauri::command]
#[specta::specta]
pub async fn approve_npc_portrait(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Character, MythicError> {
    let state = state.read().await;
    let character = CharacterRepo::get(&state.db, &character_id).await?;
    CharacterRepo::set_portrait(&state.db, &character_id, character.avatar_path.as_deref(), "approved").await
}

/// Rejects a pending NPC portrait — clears the avatar entirely (back to the
/// initial-circle placeholder) rather than leaving a rejected image in place.
#[tauri::command]
#[specta::specta]
pub async fn reject_npc_portrait(
    state: State<'_, Arc<RwLock<AppState>>>,
    character_id: String,
) -> Result<Character, MythicError> {
    let state = state.read().await;
    CharacterRepo::set_portrait(&state.db, &character_id, None, "approved").await
}

/// Returns a multi-character "cast graph" scoped to one conversation — every
/// character in that conversation's cast (gallery mains + NPCs) plus their
/// combined memories/links. Additive to (not a replacement for) the existing
/// per-character `get_memory_graph`, which spans all of one character's
/// conversations instead of all characters in one conversation.
#[tauri::command]
#[specta::specta]
pub async fn get_cast_memory_graph(
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
) -> Result<MemoryGraph, MythicError> {
    let state = state.read().await;
    MemoryRepo::get_cast_graph(&state.db, &conversation_id).await
}

/// TEMPORARY Phase-A dev command — runs the NPC detection pipeline directly
/// and synchronously (unlike the real pipeline, which always runs as a
/// swallowed-error background task) so it can be exercised and verified
/// before it's wired into live chat. Remove once Phase B wires automatic
/// triggers into `commands::chat`.
#[tauri::command]
#[specta::specta]
pub async fn debug_run_npc_detection(
    app: tauri::AppHandle,
    state: State<'_, Arc<RwLock<AppState>>>,
    conversation_id: String,
    ai_response: String,
) -> Result<(), MythicError> {
    let db = state.read().await.db.clone();
    // Fresh id each call so the cadence dedup guard never blocks a manual
    // debug invocation regardless of how many times it's been called before.
    let message_id = uuid::Uuid::new_v4().to_string();
    run_npc_detection(&db, &app, &conversation_id, &message_id, &ai_response, true).await
}
