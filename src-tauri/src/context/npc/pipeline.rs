//! Orchestrates the story-driven NPC detection pipeline: cadence/dedup gate
//! -> Stage 1 detection -> per-candidate two-pass debounce -> Stage 2 profile
//! generation -> character + cast row creation -> frontend notification.
//!
//! Every step after the initial cadence check returns a `Result`, but the
//! caller (a background `tokio::spawn`, see `commands::chat::pipeline::spawn_npc_detection`
//! in Phase B) is expected to log-and-swallow any error — this pipeline must
//! never block or fail a chat turn.

use serde_json::json;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tauri::Emitter;
use tracing::{debug, info};

use crate::commands::npc::perform_profile_refresh;
use crate::context::npc::{detector, profile_generator};
use crate::context::response_parser::resolve_character_id;
use crate::db::characters::CharacterRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::npc_candidates::NpcCandidateRepo;
use crate::db::scene_states::SceneStateRepo;
use crate::error::MythicError;
use crate::models::character::Character;
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider, resolve_model_id};

/// Number of message-completion events between periodic (non-forced)
/// detection passes, when scene_extractor's `notable_character_event` flag
/// never fires. Acts as a safety net so detection can't silently stall.
pub const NPC_DETECTION_CADENCE: i32 = 5;

pub async fn run_npc_detection(
    db: &Surreal<Db>,
    app: &tauri::AppHandle,
    conversation_id: &str,
    assistant_message_id: &str,
    ai_response: &str,
    forced: bool,
) -> Result<(), MythicError> {
    let due = NpcCandidateRepo::bump_and_check_due(
        db,
        conversation_id,
        assistant_message_id,
        forced,
        NPC_DETECTION_CADENCE,
    )
    .await?;
    if !due {
        debug!(
            "[npc_flow] Not due for conversation {} (forced={})",
            conversation_id, forced
        );
        return Ok(());
    }
    info!(
        "[npc_flow] Running detection pass for conversation {} (forced={})",
        conversation_id, forced
    );

    let provider_config = get_default_llm_provider(db).await?;
    let provider = create_rig_provider(&provider_config)?;
    // Same resolution real chat completions use — falls back to the first
    // enabled non-embedding model when the provider config has no `model`
    // field itself (e.g. OpenRouter, where the active model lives in the
    // separate enabled_models table). A naive `config.get("model")` fallback
    // to the literal string "default" gets rejected by every real provider.
    let model_id = resolve_model_id(None, &provider_config, db).await?;

    // Known names = current cast (gallery mains + already-tracked NPCs) plus
    // any candidate ever seen for this conversation, so the detector never
    // rediscovers someone it (or a prior pass) already flagged.
    let cast = ConversationCharacterRepo::list(db, conversation_id)
        .await
        .unwrap_or_default();
    let mut known_names: Vec<String> = cast.iter().map(|c| c.character_name.clone()).collect();

    // The conversation's own primary character (conversations.character_id)
    // has NO row in conversation_characters at all for a normal
    // single-character chat — that table only tracks group-cast/NPC
    // additions — so it must be fetched and included separately here, or the
    // detector never learns who the main character even is and can flag
    // them as a brand-new candidate.
    let primary_character: Option<Character> =
        match ConversationRepo::get(db, conversation_id).await {
            Ok(conv) => match conv.character_id {
                Some(char_id) => {
                    CharacterRepo::get(db, &crate::db::value_bridge::record_id_to_string(&char_id))
                        .await
                        .ok()
                }
                None => None,
            },
            Err(_) => None,
        };
    if let Some(ref pc) = primary_character {
        known_names.push(pc.name.clone());
    }

    known_names.extend(
        NpcCandidateRepo::list_known_names(db, conversation_id)
            .await
            .unwrap_or_default(),
    );

    let candidates =
        detector::detect_candidates(&provider, &model_id, ai_response, &known_names).await?;
    info!(
        "[npc_flow] Stage 1 detected {} candidate(s): {:?}",
        candidates.len(),
        candidates
            .iter()
            .map(|c| format!("{} ({})", c.name, c.tag))
            .collect::<Vec<_>>()
    );

    // (name, id) pairs for everyone already in the cast — used below to catch
    // a name variant of someone already tracked (e.g. Stage 1 reporting the
    // full "Lena Varel" after she introduces herself, when she's already
    // known under the shorter "Lena" from when she first spoke). The
    // detector's own "never re-report a KNOWN CAST name" instruction is only
    // a soft LLM prompt rule and isn't reliable for exact-string matching
    // across name variants — this is the programmatic backstop. Reuses the
    // same fuzzy (exact/case-insensitive/first-name/substring) matching
    // `resolve_character_id` already does for resolving a spoken `[Name]:`
    // marker against known cast members.
    let mut known_pairs: Vec<(String, String)> = Vec::new();
    if let Some(ref pc) = primary_character {
        known_pairs.push((
            pc.name.clone(),
            crate::db::value_bridge::record_id_to_string(&pc.id),
        ));
    }
    for c in &cast {
        known_pairs.push((
            c.character_name.clone(),
            crate::db::value_bridge::record_id_to_string(&c.character_id),
        ));
    }

    for c in &candidates {
        if let Some(existing_id) = resolve_character_id(&c.name, &known_pairs) {
            // A candidate resolving to an already-known name is NOT
            // automatically "done" — only a fully-established cast member
            // (the primary, or someone already promoted past role='transient')
            // should be excluded from the debounce entirely. A still-pending
            // placeholder (registered the moment they were first detected, so
            // they could speak right away — see `register_placeholder` below)
            // needs to keep accumulating passes just like a brand-new
            // candidate would, or `pass_count` freezes at 1 forever and
            // Stage 2's real profile generation + role promotion to 'npc'
            // never fires. That was happening for every single auto-detected
            // NPC: the very first detection registers the placeholder AND
            // its cast row, so by the *second* detection this same "already
            // known" check unconditionally short-circuited before
            // upsert_candidate (the only thing that bumps pass_count) ever
            // ran again.
            let is_primary = primary_character
                .as_ref()
                .map(|pc| crate::db::value_bridge::record_id_to_string(&pc.id) == existing_id)
                .unwrap_or(false);
            let still_pending = !is_primary
                && cast.iter().any(|m| {
                    crate::db::value_bridge::record_id_to_string(&m.character_id) == existing_id
                        && m.role == "transient"
                });

            if !still_pending {
                debug!(
                    "[npc_pipeline] '{}' resolves to an already-established cast member ({}) — not tracked for debounce",
                    c.name, existing_id
                );
                maybe_auto_refresh_placeholder(
                    db,
                    conversation_id,
                    &existing_id,
                    ai_response,
                    &provider,
                    &model_id,
                )
                .await;
                continue;
            }

            debug!(
                "[npc_pipeline] '{}' resolves to a still-pending placeholder ({}) — counting this detection toward the debounce",
                c.name, existing_id
            );
            if let Err(e) =
                NpcCandidateRepo::upsert_candidate(db, conversation_id, &c.name, &c.tag).await
            {
                debug!(
                    "[npc_pipeline] Failed to bump pass count for pending candidate '{}': {}",
                    c.name, e
                );
            }
            continue;
        }
        match NpcCandidateRepo::upsert_candidate(db, conversation_id, &c.name, &c.tag).await {
            Ok(candidate) => {
                // Register a lightweight placeholder in the cast the moment
                // a name is first tagged recurring/pivotal — the detector
                // already never returns background/transactional names at
                // all, so this tag alone is signal enough to let them
                // speak/respond in their own bubble right away, rather than
                // waiting for the full profile (Stage 2, gated on the
                // two-pass debounce below) before they can participate.
                if candidate.resulting_character_id.is_none() {
                    register_placeholder(db, app, conversation_id, &candidate).await;
                }
            }
            Err(e) => debug!(
                "[npc_pipeline] Failed to upsert candidate '{}': {}",
                c.name, e
            ),
        }
    }

    let debounced = NpcCandidateRepo::get_debounced(db, conversation_id).await?;
    if debounced.is_empty() {
        return Ok(());
    }
    info!(
        "[npc_flow] {} candidate(s) crossed the debounce threshold, generating profiles: {:?}",
        debounced.len(),
        debounced
            .iter()
            .map(|c| c.display_name.clone())
            .collect::<Vec<_>>()
    );

    // Context shared across every candidate resolved this pass.
    let scene_context = match SceneStateRepo::get(db, conversation_id).await {
        Ok(Some(state)) => serde_json::to_string(&state).ok(),
        _ => None,
    };

    let mut existing_cast: Vec<(String, String)> = Vec::new();
    if let Some(ref pc) = primary_character {
        let desc = pc
            .data
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        existing_cast.push((pc.name.clone(), desc));
    }
    for member in &cast {
        if let Ok(character) = CharacterRepo::get(
            db,
            &crate::db::value_bridge::record_id_to_string(&member.character_id),
        )
        .await
        {
            let desc = character
                .data
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            existing_cast.push((character.name, desc));
        }
    }

    for candidate in debounced {
        // Exclude the candidate's own placeholder (already registered above,
        // possibly a pass or two ago) from its own "existing cast" context —
        // otherwise the profile generator gets told to "stay consistent
        // with" a placeholder entry describing itself.
        let filtered_existing_cast: Vec<(String, String)> = existing_cast
            .iter()
            .filter(|(name, _)| !name.eq_ignore_ascii_case(&candidate.display_name))
            .cloned()
            .collect();

        let profile = match profile_generator::generate_profile(
            &provider,
            &model_id,
            &candidate.display_name,
            ai_response,
            scene_context.as_deref(),
            &filtered_existing_cast,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                debug!(
                    "[npc_pipeline] Profile generation failed for '{}': {}",
                    candidate.display_name, e
                );
                continue;
            }
        };

        let data = serde_json::to_value(&profile).unwrap_or_default();

        // A placeholder was already registered in the cast when this name
        // was first detected — fill in its real profile rather than
        // creating a second character.
        let character = match &candidate.resulting_character_id {
            Some(existing) => {
                match CharacterRepo::update_npc_profile(
                    db,
                    &crate::db::value_bridge::record_id_to_string(existing),
                    &profile.name,
                    data,
                )
                .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        debug!(
                            "[npc_pipeline] Failed to update placeholder profile for '{}': {}",
                            profile.name, e
                        );
                        continue;
                    }
                }
            }
            None => {
                // Safety net — placeholder registration failed earlier for
                // some reason; fall back to creating a character now.
                match CharacterRepo::create_npc(db, &profile.name, data).await {
                    Ok(c) => c,
                    Err(e) => {
                        debug!(
                            "[npc_pipeline] Failed to create NPC character '{}': {}",
                            profile.name, e
                        );
                        continue;
                    }
                }
            }
        };

        let char_id = crate::db::value_bridge::record_id_to_string(&character.id);
        match &candidate.resulting_character_id {
            Some(_) => {
                // A placeholder already exists in the cast (role 'transient')
                // — promote it now that a real profile has been written.
                if let Err(e) =
                    ConversationCharacterRepo::set_role(db, conversation_id, &char_id, "npc").await
                {
                    debug!(
                        "[npc_pipeline] Failed to promote '{}' to npc: {}",
                        character.name, e
                    );
                }
            }
            None => {
                if let Err(e) = ConversationCharacterRepo::add(
                    db,
                    conversation_id,
                    &char_id,
                    &character.name,
                    "npc",
                    50,
                )
                .await
                {
                    debug!(
                        "[npc_pipeline] Failed to add NPC '{}' to cast: {}",
                        character.name, e
                    );
                }
            }
        }
        if let Err(e) = NpcCandidateRepo::mark_created(
            db,
            &crate::db::value_bridge::record_id_to_string(&candidate.id),
            &char_id,
        )
        .await
        {
            debug!(
                "[npc_pipeline] Failed to mark candidate '{}' created: {}",
                candidate.display_name, e
            );
        }

        info!(
            "[npc_flow] Filled in full profile for '{}' ({}) in conversation {}",
            character.name, char_id, conversation_id
        );

        let _ = app.emit(
            "npc_created",
            json!({ "conversation_id": conversation_id, "character": character }),
        );
    }

    Ok(())
}

/// The two exact placeholder strings written by `register_placeholder` and
/// `register_transient_speaker` respectively — a character still carrying
/// one of these has never had a real profile written, which is exactly the
/// case the automatic "Refresh from Story" trigger exists to fix.
const PLACEHOLDER_DESCRIPTIONS: [&str; 2] = [
    "Just arrived in the story — their role isn't clear yet.",
    "Just spoke for the first time — their role in the story isn't clear yet.",
];

/// If `character_id` still carries one of the known placeholder
/// descriptions (i.e. Stage 2 profile generation never got to them — they
/// were only ever synchronously registered as a transient speaker, or their
/// Stage 2 pass previously failed), refreshes it now using the story
/// content this detection pass already has in hand. Best-effort: swallows
/// and logs its own errors, matching every other step in this pipeline —
/// never blocks or fails the chat turn it's running alongside.
async fn maybe_auto_refresh_placeholder(
    db: &Surreal<Db>,
    conversation_id: &str,
    character_id: &str,
    ai_response: &str,
    provider: &crate::providers::unified::RigProvider,
    model_id: &str,
) {
    let character = match CharacterRepo::get(db, character_id).await {
        Ok(c) => c,
        Err(_) => return,
    };
    let description = character
        .data
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !PLACEHOLDER_DESCRIPTIONS.contains(&description.trim()) {
        return;
    }

    match perform_profile_refresh(
        db,
        character_id,
        conversation_id,
        Some(ai_response),
        provider,
        model_id,
        None,
    )
    .await
    {
        Ok(result) => {
            info!(
                "[npc_flow] Auto-refreshed still-placeholder profile for '{}' ({}) in conversation {} (scope={})",
                character.name, character_id, conversation_id, result.scope
            );
        }
        Err(e) => {
            debug!(
                "[npc_pipeline] Auto profile refresh failed for '{}': {}",
                character.name, e
            );
        }
    }
}

/// Checks whether `name` already resolves (via the same fuzzy exact/
/// case-insensitive/first-name/substring matching used elsewhere to resolve
/// a spoken `[Name]:` marker) to an existing member of this conversation's
/// cast — the primary character or any `conversation_characters` row,
/// transient or otherwise. Every character-creation path in this module
/// checks this first, so it stays self-defending against duplicates no
/// matter which of the several independent triggers (Stage 1 detection, a
/// synchronous marker registration, ...) happens to fire first for the same
/// person.
async fn find_existing_cast_member(
    db: &Surreal<Db>,
    conversation_id: &str,
    name: &str,
) -> Option<(String, String)> {
    let cast = ConversationCharacterRepo::list(db, conversation_id)
        .await
        .ok()?;
    let mut known_pairs: Vec<(String, String)> = Vec::new();
    if let Ok(conv) = ConversationRepo::get(db, conversation_id).await {
        if let Some(char_id) = conv.character_id {
            if let Ok(primary) =
                CharacterRepo::get(db, &crate::db::value_bridge::record_id_to_string(&char_id))
                    .await
            {
                known_pairs.push((
                    primary.name,
                    crate::db::value_bridge::record_id_to_string(&char_id),
                ));
            }
        }
    }
    for c in &cast {
        known_pairs.push((
            c.character_name.clone(),
            crate::db::value_bridge::record_id_to_string(&c.character_id),
        ));
    }
    let id = resolve_character_id(name, &known_pairs)?;
    let matched_name = known_pairs
        .iter()
        .find(|(_, i)| *i == id)
        .map(|(n, _)| n.clone())?;
    Some((id, matched_name))
}

/// Registers a lightweight placeholder character for a just-detected
/// candidate — name only, generic description — so it can speak/respond in
/// the cast immediately, before its real profile is generated. Best-effort:
/// swallows and logs its own errors, matching every other step here.
async fn register_placeholder(
    db: &Surreal<Db>,
    app: &tauri::AppHandle,
    conversation_id: &str,
    candidate: &crate::models::npc_candidate::NpcCandidate,
) {
    if let Some((existing_id, existing_name)) =
        find_existing_cast_member(db, conversation_id, &candidate.display_name).await
    {
        debug!(
            "[npc_pipeline] '{}' already matches existing cast member '{}' ({}) — linking instead of creating a duplicate",
            candidate.display_name, existing_name, existing_id
        );
        if let Err(e) = NpcCandidateRepo::set_placeholder_character(
            db,
            &crate::db::value_bridge::record_id_to_string(&candidate.id),
            &existing_id,
        )
        .await
        {
            debug!(
                "[npc_pipeline] Failed to link '{}' to existing character: {}",
                candidate.display_name, e
            );
        }
        return;
    }

    let placeholder_data = serde_json::json!({
        "name": candidate.display_name,
        "description": "Just arrived in the story — their role isn't clear yet.",
        "personality": "",
        "scenario": "",
        "first_mes": "",
        "tags": [],
    });

    let character =
        match CharacterRepo::create_npc(db, &candidate.display_name, placeholder_data).await {
            Ok(c) => c,
            Err(e) => {
                debug!(
                    "[npc_pipeline] Failed to create placeholder for '{}': {}",
                    candidate.display_name, e
                );
                return;
            }
        };

    let char_id = crate::db::value_bridge::record_id_to_string(&character.id);
    if let Err(e) = ConversationCharacterRepo::add(
        db,
        conversation_id,
        &char_id,
        &character.name,
        "transient",
        30,
    )
    .await
    {
        debug!(
            "[npc_pipeline] Failed to add placeholder '{}' to cast: {}",
            character.name, e
        );
    }
    if let Err(e) = NpcCandidateRepo::set_placeholder_character(
        db,
        &crate::db::value_bridge::record_id_to_string(&candidate.id),
        &char_id,
    )
    .await
    {
        debug!(
            "[npc_pipeline] Failed to link placeholder for '{}': {}",
            candidate.display_name, e
        );
    }

    info!(
        "[npc_flow] Registered placeholder for '{}' ({}) in conversation {} — can respond in the cast now",
        character.name, char_id, conversation_id
    );

    let _ = app.emit(
        "npc_created",
        json!({ "conversation_id": conversation_id, "character": character }),
    );
}

/// Synchronously registers a brand-new speaker the moment the LLM voices
/// them via their own `[Name]:` marker but they aren't in the cast yet — no
/// detector call, no LLM call, just two DB writes (create the character,
/// add her to `conversation_characters` as `role: 'transient'`), safe to
/// `await` inline in the streaming Done-handler between "response finished"
/// and "segments get saved," so she gets her own bubble in this same turn.
///
/// Also seeds/bumps her `npc_candidates` row so she can still reach the
/// normal two-pass debounce and get promoted (`'transient' -> 'npc'`) with a
/// real profile later — without this, `detector.rs`'s "never re-report a
/// name already in the known cast" instruction would mean Stage 1 never
/// looks at her again, leaving her stuck as a placeholder forever.
///
/// Returns `(character_id, character_name)` on success so the caller can
/// fold her into its own in-memory resolution list immediately (needed so a
/// second appearance of the same new name later in the SAME response
/// resolves against this row instead of creating a duplicate).
pub async fn register_transient_speaker(
    db: &Surreal<Db>,
    app: &tauri::AppHandle,
    conversation_id: &str,
    name: &str,
) -> Option<(String, String)> {
    if let Some((existing_id, existing_name)) =
        find_existing_cast_member(db, conversation_id, name).await
    {
        debug!(
            "[npc_pipeline] '{}' already matches existing cast member '{}' ({}) — reusing instead of creating a duplicate",
            name, existing_name, existing_id
        );
        return Some((existing_id, existing_name));
    }

    let placeholder_data = serde_json::json!({
        "name": name,
        "description": "Just spoke for the first time — their role in the story isn't clear yet.",
        "personality": "",
        "scenario": "",
        "first_mes": "",
        "tags": [],
    });

    let character = match CharacterRepo::create_npc(db, name, placeholder_data).await {
        Ok(c) => c,
        Err(e) => {
            debug!(
                "[npc_pipeline] Failed to register transient speaker '{}': {}",
                name, e
            );
            return None;
        }
    };
    let char_id = crate::db::value_bridge::record_id_to_string(&character.id);

    if let Err(e) = ConversationCharacterRepo::add(
        db,
        conversation_id,
        &char_id,
        &character.name,
        "transient",
        30,
    )
    .await
    {
        debug!(
            "[npc_pipeline] Failed to add transient speaker '{}' to cast: {}",
            character.name, e
        );
    }

    if let Ok(candidate) =
        NpcCandidateRepo::upsert_candidate(db, conversation_id, name, "recurring").await
    {
        if let Err(e) = NpcCandidateRepo::set_placeholder_character(
            db,
            &crate::db::value_bridge::record_id_to_string(&candidate.id),
            &char_id,
        )
        .await
        {
            debug!(
                "[npc_pipeline] Failed to link transient speaker '{}' to its candidate row: {}",
                name, e
            );
        }
    }

    info!(
        "[npc_flow] Registered transient speaker '{}' ({}) in conversation {} — speaking this turn",
        character.name, char_id, conversation_id
    );

    let _ = app.emit(
        "npc_created",
        json!({ "conversation_id": conversation_id, "character": character }),
    );

    Some((char_id, character.name))
}
