//! Builds the full LLM prompt from character data, lorebook, memories,
//! emotional state, scene state, and token-budgeted conversation history.
//!
//! Extracted out of `commands::chat` — this is pure prompt-assembly logic
//! (no `State`/`AppState`, not a `#[tauri::command]`) that orchestrates the
//! other `context::` modules, so it belongs here rather than alongside the
//! Tauri command handlers that call it.

use tracing::info;

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::context::budget::ContextBudget;
use crate::context::rag::{query_relevant_context, query_relevant_memories};
use crate::context::tokenizer::count_message_tokens;
use crate::context::window::apply_sliding_window;

use crate::db::character_state::CharacterStateRepo;
use crate::db::characters::CharacterRepo;
use crate::db::conversation_characters::ConversationCharacterRepo;
use crate::db::conversations::ConversationRepo;
use crate::db::lorebook::LorebookRepo;
use crate::db::memories::MemoryRepo;
use crate::db::messages::MessageRepo;
use crate::db::personas::PersonaRepo;
use crate::db::scene_states::SceneStateRepo;
use crate::db::summaries::SummaryRepo;
use crate::error::MythicError;
use crate::models::conversation::{ChatMessage, MessageRole};
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider};

/// Statistics about the context window for observability.
/// Returned alongside the prompt so callers can log/surface token usage.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ContextStats {
    /// Total token budget for the context window.
    #[specta(type = u32)]
    pub total_budget: usize,
    /// Tokens used by fixed layers (system, character, lorebook, memories, emotion, PHI).
    #[specta(type = u32)]
    pub fixed_tokens: usize,
    /// Tokens used by conversation history (after sliding window).
    #[specta(type = u32)]
    pub history_tokens: usize,
    /// Tokens used by the rolling summary (0 if no summary yet).
    #[specta(type = u32)]
    pub summary_tokens: usize,
    /// Total messages in the full conversation branch.
    #[specta(type = u32)]
    pub total_messages: usize,
    /// Messages included in the sliding window.
    #[specta(type = u32)]
    pub included_messages: usize,
    /// Messages evicted (not sent to the LLM).
    #[specta(type = u32)]
    pub evicted_messages: usize,
    /// Tokens used by RAG-retrieved context (0 if RAG disabled or no results).
    #[specta(type = u32)]
    pub rag_tokens: usize,
}

/// Formats a list of memory facts as a bulleted block under `header` —
/// the shared shape behind every memory injection in `build_prompt()`
/// (per-character, semantic, recency-fallback, and no-character paths).
fn format_memory_block(header: &str, facts: &[String]) -> String {
    format!(
        "{}\n{}",
        header,
        facts.iter().map(|f| format!("• {}", f)).collect::<Vec<_>>().join("\n"),
    )
}

/// Whether `keyword` appears in `corpus` on a word boundary — i.e. not as a
/// substring buried inside a longer word. Both arguments are expected
/// already lowercased by the caller (this does no case handling itself).
/// A plain `corpus.contains(keyword)` used to let a short lorebook keyword
/// like "cat" fire on "catastrophe", triggering unrelated lore constantly.
/// Multi-word keys (e.g. "new york") are supported — the whole phrase must
/// be bounded by non-alphanumeric characters (or start/end of string) on
/// both sides, not each word individually.
fn keyword_matches_at_word_boundary(corpus: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return false;
    }
    for (start, matched) in corpus.match_indices(keyword) {
        let end = start + matched.len();
        let before_ok = corpus[..start].chars().next_back().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        let after_ok = corpus[end..].chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Case-insensitive `{{user}}` -> display-name substitution, applied to
/// character-card text fields. Uses the active persona's name when the
/// conversation has one selected; otherwise falls back to the generic
/// `"User"` token so `{{user}}`-authored cards still read naturally without
/// requiring anyone to set up a persona. A no-op (returns the input
/// unchanged) only when the text contains no `{{user}}` marker at all.
fn substitute_user_macro(text: &str, persona_name: Option<&str>) -> String {
    if !text.to_lowercase().contains("{{user}}") {
        return text.to_string();
    }
    let name = persona_name.unwrap_or("User");
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let lower = rest.to_lowercase();
        match lower.find("{{user}}") {
            Some(idx) => {
                result.push_str(&rest[..idx]);
                result.push_str(name);
                rest = &rest[idx + "{{user}}".len()..];
            }
            None => {
                result.push_str(rest);
                break;
            }
        }
    }
    result
}

/// Builds the full prompt by combining system prompt, character data,
/// and conversation history. Now token-budgeted via sliding window.
// The early `return` inside the multi-char branch below is a real early
// exit (skips the `else` branch entirely) — converting it to a bare tail
// expression would only make the enclosing statement-position block's
// value silently discarded, not actually return from the function.
// Verified (and previously broken by exactly this) — left as `return`.
#[allow(clippy::needless_return)]
pub(crate) async fn build_prompt(
    db: &Surreal<Db>,
    conversation_id: &str,
    up_to_message_id: &str,
    user_system_prompt: Option<&str>,
    post_history_instructions: Option<&str>,
    context_budget: &ContextBudget,
) -> Result<(Vec<ChatMessage>, ContextStats), MythicError> {
    let mut prompt = Vec::new();

    // Inject the user's global system prompt first (from Settings)
    if let Some(sys) = user_system_prompt {
        let trimmed = sys.trim();
        if !trimmed.is_empty() {
            prompt.push(ChatMessage {
                role: MessageRole::System,
                content: trimmed.to_string(),
            });
        }
    }

    // Get the character and memory scope associated with this conversation
    let conv = ConversationRepo::get(db, conversation_id).await.ok();

    let (character_id, memory_scope) = match conv {
        Some(ref c) => {
            let char_id = c.character_id.as_ref().map(|t| t.id.to_raw());
            (char_id, c.memory_scope.clone())
        }
        None => (None, "character".to_string()),
    };

    // The conversation's active persona (the user's own stand-in), if any —
    // drives {{user}} macro substitution in character-card text below
    // (falls back to the generic "User" token when no persona is set) plus
    // an "About the User" context block (only shown when a persona IS set —
    // there's nothing persona-specific to say otherwise).
    let persona = match conv.as_ref().and_then(|c| c.persona_id.as_ref()) {
        Some(pid) => PersonaRepo::get(db, &pid.id.to_raw()).await.ok(),
        None => None,
    };
    let persona_name = persona.as_ref().map(|p| p.name.as_str());

    // ── Check for multi-character mode ──
    // "Genuine" multi-character mode requires at least one OTHER cast member
    // (not the conversation's own primary character) who is either manually
    // added (role: 'secondary' — always a real Gallery character, since the
    // Group Cast add-picker only lists Gallery characters) or an
    // auto-detected NPC the user has actually promoted to their Gallery
    // (origin: 'gallery'). A merely-transient or confirmed-but-unpromoted
    // NPC does NOT flip this — the conversation stays in solo mode (with its
    // own "may voice others" permission, see the Roleplay Directive below).
    // The heavier Group Scene Directive only kicks in once the user has
    // actually decided this is a deliberate multi-character story, not just
    // "someone spoke once" or "the story flagged them as significant."
    let conv_chars = ConversationCharacterRepo::list(db, conversation_id).await.unwrap_or_default();
    let active_conv_chars: Vec<_> = conv_chars.iter().filter(|c| c.is_active).collect();
    let mut is_multi_char = false;
    for c in &active_conv_chars {
        let char_id_raw = c.character_id.id.to_raw();
        if character_id.as_deref() == Some(char_id_raw.as_str()) {
            continue; // the primary herself never counts toward "genuine multi-char"
        }
        if c.role == "primary" || c.role == "secondary" {
            is_multi_char = true;
            break;
        }
        // role is 'npc' or 'transient' — only counts once actually promoted
        if let Ok(character) = CharacterRepo::get(db, &char_id_raw).await {
            if character.origin == "gallery" {
                is_multi_char = true;
                break;
            }
        }
    }

    if is_multi_char {
        // ── Multi-character mode: inject all character cards ──
        info!("[build_prompt] Multi-char mode: {} active characters", active_conv_chars.len());

        // The primary character may have no conversation_characters row at
        // all — that table is only ever populated by a manual Group Cast
        // add, or by this app's own auto-detected-NPC pipeline; a plain
        // solo conversation that later gains a promoted NPC never gets the
        // primary migrated in on its own. Without this, "genuine multi-char"
        // mode (see above) could fire with a prompt that describes every
        // *other* cast member but never the primary character herself.
        let primary_has_row = active_conv_chars.iter()
            .any(|c| Some(c.character_id.id.to_raw().as_str()) == character_id.as_deref());
        if !primary_has_row {
            if let Some(ref char_id) = character_id {
                if let Ok(character) = CharacterRepo::get(db, char_id).await {
                    let card = &character.data;
                    let mut parts = Vec::new();
                    parts.push(format!("[Primary Character — {}]", character.name));
                    if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                        let sys = substitute_user_macro(sys, persona_name);
                        if !sys.is_empty() { parts.push(sys); }
                    }
                    if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                        let desc = substitute_user_macro(desc, persona_name);
                        if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                    }
                    if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                        let personality = substitute_user_macro(personality, persona_name);
                        if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                    }
                    if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                        let scenario = substitute_user_macro(scenario, persona_name);
                        if !scenario.is_empty() { parts.push(format!("Scenario: {}", scenario)); }
                    }
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: parts.join("\n"),
                    });
                }
            }
        }

        for conv_char in &active_conv_chars {
            let char_id_raw = conv_char.character_id.id.to_raw();
            if let Ok(character) = CharacterRepo::get(db, &char_id_raw).await {
                let card = &character.data;
                let name = &conv_char.character_name;
                let role = &conv_char.role;

                match role.as_str() {
                    "primary" => {
                        // Full character card for primary
                        let mut parts = Vec::new();
                        parts.push(format!("[Primary Character — {}]", name));

                        if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                            let sys = substitute_user_macro(sys, persona_name);
                            if !sys.is_empty() { parts.push(sys); }
                        }
                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            let desc = substitute_user_macro(desc, persona_name);
                            if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                        }
                        if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                            let personality = substitute_user_macro(personality, persona_name);
                            if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                        }
                        if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                            let scenario = substitute_user_macro(scenario, persona_name);
                            if !scenario.is_empty() { parts.push(format!("Scenario: {}", scenario)); }
                        }

                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: parts.join("\n"),
                        });
                    }
                    "secondary" => {
                        // Condensed card for secondary characters
                        let mut parts = Vec::new();
                        parts.push(format!("[Character — {}]", name));

                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            let desc = substitute_user_macro(desc, persona_name);
                            if !desc.is_empty() { parts.push(format!("Description: {}", desc)); }
                        }
                        if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                            let personality = substitute_user_macro(personality, persona_name);
                            if !personality.is_empty() { parts.push(format!("Personality: {}", personality)); }
                        }
                        parts.push(format!("(Talkativeness: {}/100)", conv_char.talkativeness));

                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: parts.join("\n"),
                        });
                    }
                    _ => {
                        // Minimal card for NPCs
                        let mut parts = Vec::new();
                        parts.push(format!("[NPC — {}]", name));

                        if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                            let desc = substitute_user_macro(desc, persona_name);
                            if !desc.is_empty() { parts.push(desc); }
                        }
                        parts.push("(Minor character — respond briefly when directly involved)".to_string());

                        prompt.push(ChatMessage {
                            role: MessageRole::System,
                            content: parts.join("\n"),
                        });
                    }
                }
            }
        }

        // ── Group Scene Directive ──
        let mut char_names: Vec<String> = active_conv_chars.iter()
            .map(|c| c.character_name.clone())
            .collect();
        if !primary_has_row {
            if let Some(ref char_id) = character_id {
                if let Ok(character) = CharacterRepo::get(db, char_id).await {
                    char_names.insert(0, character.name);
                }
            }
        }
        let group_directive = format!(
            "[Group Scene Directive]\n\
             You are narrating a group roleplay scene. Multiple characters are present: {}.\n\
             When responding, write for ALL characters who are relevant to the current moment.\n\n\
             CRITICAL — Response format: You MUST prefix EVERY character's section with their full name \
             in square brackets followed by a colon. This is mandatory and must never be omitted.\n\n\
             Example format:\n\
             [Aria Silverleaf]: *Aria's actions and dialogue here*\n\n\
             [Finn Shadowcloak]: *Finn's actions and dialogue here*\n\n\
             Rules:\n\
             - EVERY section of your response MUST start with [FullCharacterName]: — never write \
             character dialogue or actions without this prefix tag\n\
             - When one character speaks TO or about another character present in the scene, \
             the addressed character MUST respond in the same generation. Do not wait for {{{{user}}}} input \
             between character exchanges\n\
             - Write natural back-and-forth dialogue between characters when the scene calls for it\n\
             - Normally only respond as characters listed above, never as {{{{user}}}}. The one exception: if the \
             current scene lists another character present who ISN'T in the list above, and they're directly \
             addressed, commanded, or the moment truly calls for their own reaction, you may voice them too using \
             the same [FullCharacterName]: format — never invent a name that isn't listed as present in the scene\n\
             - Each character must maintain their distinct voice, personality, and speech patterns\n\
             - Characters with higher talkativeness should respond more frequently and at greater length\n\
             - If a character has nothing meaningful to add, they may be omitted — but characters who are \
             directly addressed, challenged, or asked a question must always respond\n\
             - Never generate {{{{user}}}}'s dialogue or actions",
            char_names.join(", ")
        );
        prompt.push(ChatMessage {
            role: MessageRole::System,
            content: group_directive,
        });
    } else if let Some(ref char_id) = character_id {
        // ── Single-character mode (existing behavior) ──
        if let Ok(character) = CharacterRepo::get(db, char_id).await {
            let card = &character.data;
            let mut system_parts = Vec::new();

            // Who else is ALREADY registered in this conversation's cast
            // (excluding the primary herself) — the authoritative source for
            // "who may be voiced," used INSTEAD of the scene-state
            // extractor's "Present" list. That list is populated by a
            // separate, best-effort background LLM call that runs only
            // after each response and has repeatedly failed to pick up a
            // just-introduced/just-addressed character for several turns in
            // a row (observed directly: a registered cast member absent from
            // "Present" for 3+ consecutive extraction passes) — which, under
            // the old "never invent a name not listed as present" wording,
            // silently blocked her from ever being voiced even once
            // everyone (the user included) was already treating her as
            // present. A name already in the cast table doesn't need the
            // scene extractor's permission to exist.
            let known_others: Vec<String> = active_conv_chars.iter()
                .filter(|c| character_id.as_deref() != Some(c.character_id.id.to_raw().as_str()))
                .map(|c| c.character_name.clone())
                .collect();
            let known_others_clause = if known_others.is_empty() {
                String::new()
            } else {
                format!(" Already part of this story's cast: {}.", known_others.join(", "))
            };

            // Baseline roleplay behavior contract — established first so it
            // holds regardless of whether the character card defines its own
            // system_prompt. Without this, some models (observed with
            // Nemotron) default to narrating/analyzing the scene from an
            // outside perspective ("The user is greeting Aria...") instead
            // of actually responding in character from turn one.
            system_parts.push(format!(
                "[Roleplay Directive]\n\
                 You are {name}. Fully embody this character and respond only as {name} — never describe, \
                 narrate, or speak for the user, and never write from an outside narrator's perspective. \
                 Write immersive narrative prose: actions and body language in *asterisks*, spoken dialogue in \"quotes\". \
                 Stay grounded in {name}'s own personality, voice, knowledge, and worldview at all times. \
                 Never break character, add out-of-character commentary, disclaimers, or meta-analysis — you ARE {name} \
                 living this scene, not an assistant describing it. \
                 Do not summarize, use bullet points, or format like an AI assistant. Drive the scene forward naturally: \
                 react, initiate, and let the story progress rather than passively restating what the user just said.\n\n\
                 [Other Characters Present] {name} is not the only person in this scene.{known_others_clause} RULE: if \
                 another character — someone already established in this story, or someone the current scene clearly \
                 introduces — is directly addressed by name, spoken to, commanded, or asked a direct question, you MUST \
                 voice their response in this SAME reply — never stop and wait for {{{{user}}}}'s next message before \
                 they answer. Prefix their section with their full name in brackets, exactly like this:\n\n\
                 [FullCharacterName]: *their action* \"their dialogue\"\n\n\
                 For anyone present who ISN'T directly addressed or relevant to this exact moment, stay silent about \
                 them — {name}'s own voice is still the default and should make up most of your response. Never \
                 invent a name with no basis anywhere in this story; never use this to speak for {{{{user}}}}; if \
                 truly no one else was addressed, write only as {name} and use no bracket markers at all.",
                name = character.name,
                known_others_clause = known_others_clause,
            ));

            // Character system prompt
            if let Some(sys) = card.get("system_prompt").and_then(|v| v.as_str()) {
                let sys = substitute_user_macro(sys, persona_name);
                if !sys.is_empty() {
                    system_parts.push(sys);
                }
            }

            // Character description
            if let Some(desc) = card.get("description").and_then(|v| v.as_str()) {
                let desc = substitute_user_macro(desc, persona_name);
                if !desc.is_empty() {
                    system_parts.push(format!("Character Description:\n{}", desc));
                }
            }

            // Personality
            if let Some(personality) = card.get("personality").and_then(|v| v.as_str()) {
                let personality = substitute_user_macro(personality, persona_name);
                if !personality.is_empty() {
                    system_parts.push(format!("Personality:\n{}", personality));
                }
            }

            // Scenario
            if let Some(scenario) = card.get("scenario").and_then(|v| v.as_str()) {
                let scenario = substitute_user_macro(scenario, persona_name);
                if !scenario.is_empty() {
                    system_parts.push(format!("Scenario:\n{}", scenario));
                }
            }

            if !system_parts.is_empty() {
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: system_parts.join("\n\n"),
                });
            }
        }
    }

    // ── About the User ──
    // Injected once, right after the character card(s) — tells the model
    // who it's actually roleplaying with when the user has an active
    // persona selected. A complete no-op when no persona is selected.
    if let Some(ref p) = persona {
        let desc = p.data.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let personality = p.data.get("personality").and_then(|v| v.as_str()).unwrap_or("");
        let mut about = format!("[About the User]\nYou are speaking with {}.", p.name);
        if !desc.is_empty() {
            about.push_str(&format!(" {}", desc));
        }
        if !personality.is_empty() {
            about.push_str(&format!(" {}", personality));
        }
        prompt.push(ChatMessage {
            role: MessageRole::System,
            content: about,
        });
    }

    // Add lorebook entries — fetch entries for every active character in
    // this conversation (not just the primary), then filter in Rust. A
    // conversation with e.g. Aria as primary and Lena as a cast member
    // previously only ever considered Aria's lorebook — Lena's entries were
    // silently never eligible for injection no matter how well-triggered
    // their keywords were.
    if let Some(ref char_id) = character_id {
        let mut lorebook_char_ids: Vec<String> = vec![char_id.clone()];
        for c in &active_conv_chars {
            let id = c.character_id.id.to_raw();
            if !lorebook_char_ids.contains(&id) {
                lorebook_char_ids.push(id);
            }
        }
        let mut all_entries: Vec<crate::models::lorebook::LorebookEntry> = Vec::new();
        {
            let mut seen_ids = std::collections::HashSet::new();
            for cid in &lorebook_char_ids {
                if let Ok(entries) = LorebookRepo::list(db, cid).await {
                    for entry in entries {
                        // A global entry (character_id IS NONE) is returned
                        // for every character queried — dedupe by id so it
                        // isn't injected once per cast member.
                        let entry_id = entry.id.id.to_raw();
                        if seen_ids.insert(entry_id) {
                            all_entries.push(entry);
                        }
                    }
                }
            }
            // Each per-character fetch is individually ORDER BY priority
            // DESC, insertion_order ASC, but merging multiple characters'
            // results loses that global ordering — re-sort so the
            // token-budget cutoff below still fills in true priority order
            // across the whole cast, not "all of character A's entries,
            // then all of character B's" regardless of relative priority.
            all_entries.sort_by(|a, b| {
                b.priority.cmp(&a.priority).then(a.insertion_order.cmp(&b.insertion_order))
            });
        }
        {
            // Collected first, THEN pushed to `prompt` subject to
            // max_lorebook_tokens below — previously every matching entry
            // was pushed unconditionally, with no cap at all (unlike the
            // memories layer just below, which has always enforced
            // context_budget.max_memory_tokens), so enough always-active/
            // triggered entries could blow the whole context budget.
            // `all_entries` is sorted by priority DESC, insertion_order ASC
            // above, so filling in that order and stopping once the budget's
            // spent keeps the most important entries.
            let mut lorebook_messages: Vec<ChatMessage> = Vec::new();

            // Always-active entries
            for entry in all_entries.iter() {
                if entry.enabled && entry.always_active {
                    lorebook_messages.push(ChatMessage {
                        role: MessageRole::System,
                        content: entry.content.clone(),
                    });
                }
            }

            // Keyword-triggered entries will be processed after we build the message chain
            // (we need the chain to scan for keywords)

            // Walk the message tree using get_branch (returns root→leaf order)
            let branch = MessageRepo::get_branch(db, up_to_message_id).await?;
            let chain: Vec<ChatMessage> = branch.iter().map(|m| {
                ChatMessage {
                    role: match m.role {
                        MessageRole::User => MessageRole::User,
                        MessageRole::Assistant => MessageRole::Assistant,
                        MessageRole::System => MessageRole::System,
                    },
                    content: m.content.clone(),
                }
            }).collect();

            // Keyword-triggered lorebook entries: scan recent messages for matching keywords
            let keyword_entries: Vec<&crate::models::lorebook::LorebookEntry> = all_entries.iter()
                .filter(|e| e.enabled && !e.always_active && !e.keys.is_empty())
                .collect();

            if !keyword_entries.is_empty() {
                // Build a search corpus from the last N messages (for performance)
                let scan_depth = chain.len().min(20);
                let corpus: String = chain[chain.len().saturating_sub(scan_depth)..]
                    .iter()
                    .map(|m| m.content.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ");

                for entry in keyword_entries {
                    // keys is already Vec<String> — no JSON parsing needed.
                    // Word-boundary match, not a raw substring check — the
                    // latter let a short keyword like "cat" fire on
                    // "catastrophe", triggering unrelated lore constantly.
                    let triggered = entry.keys
                        .iter()
                        .map(|k| k.to_lowercase())
                        .filter(|k| !k.is_empty())
                        .any(|keyword| keyword_matches_at_word_boundary(&corpus, &keyword));

                    if triggered {
                        lorebook_messages.push(ChatMessage {
                            role: MessageRole::System,
                            content: entry.content.clone(),
                        });
                    }
                }
            }

            let mut lorebook_tokens_used = 0usize;
            for msg in lorebook_messages {
                let tokens = count_message_tokens(&msg);
                if lorebook_tokens_used + tokens > context_budget.max_lorebook_tokens {
                    break;
                }
                lorebook_tokens_used += tokens;
                prompt.push(msg);
            }

            // ── Inject saved memories as a persistent context layer ──
            // Two retrieval modes:
            // 1. Semantic: query vector DB for memories relevant to current message (preferred)
            // 2. Fallback: recency-ordered list if no memory embeddings exist yet
            //
            // Placement: after lorebook, before emotional state — the "knowledge layer".
            // Token-capped to context_budget.max_memory_tokens.
            if memory_scope != "none" {
                // Get the last user message for semantic query
                let last_user_content = chain.iter().rev()
                    .find(|m| m.role == MessageRole::User)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();

                if is_multi_char {
                    // ── Multi-character memory injection ──
                    // Each character gets their own attributed memory block
                    for conv_char in &active_conv_chars {
                        let cc_char_id = conv_char.character_id.id.to_raw();
                        let char_memories = MemoryRepo::list_for_character_in_conv(
                            db, &cc_char_id, conversation_id,
                        ).await.unwrap_or_default();

                        if !char_memories.is_empty() {
                            let facts: Vec<String> = char_memories.iter()
                                .take(10) // cap per character
                                .map(|m| m.content.clone())
                                .collect();

                            let memory_block = format_memory_block(
                                &format!("[{}'s Memories]", conv_char.character_name),
                                &facts,
                            );
                            prompt.push(ChatMessage {
                                role: MessageRole::System,
                                content: memory_block,
                            });
                        }
                    }
                } else {
                    // ── Single-character memory injection (existing behavior) ──
                    let mut memory_facts: Vec<String> = Vec::new();

                    // Try semantic retrieval first
                    if !last_user_content.is_empty() {
                        if let Ok(pc) = get_default_llm_provider(db).await {
                            if let Ok(provider) = create_rig_provider(&pc) {
                                let embed_model = pc.config
                                    .get("embedding_model")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("text-embedding-3-small");
                                // memory_scope = "conversation" means the user asked for this
                                // story's memories to stay isolated — pass the conversation_id
                                // through so retrieval respects that instead of always
                                // searching this character's memories across every
                                // conversation they've ever appeared in.
                                let memory_scope_conv_id = if memory_scope == "conversation" {
                                    Some(conversation_id)
                                } else {
                                    None
                                };
                                if let Ok(results) = query_relevant_memories(
                                    db, &provider, embed_model,
                                    char_id, &last_user_content,
                                    10,   // top 10 relevant memories
                                    0.4,  // lower threshold — facts are short
                                    memory_scope_conv_id,
                                ).await {
                                    if !results.is_empty() {
                                        memory_facts = results.iter()
                                            .map(|r| r.content.clone())
                                            .collect();
                                        info!("[build_prompt] Semantic memory retrieval: {} memories", memory_facts.len());
                                    }
                                }
                            }
                        }
                    }

                    // Fallback: recency-ordered list if semantic retrieval yielded nothing
                    if memory_facts.is_empty() {
                        let memory_list = if memory_scope == "character" {
                            MemoryRepo::list(db, Some(char_id), None).await.unwrap_or_default()
                        } else {
                            // Fix: conversation scope now includes canon memories
                            MemoryRepo::list_with_canon(db, conversation_id, char_id).await.unwrap_or_default()
                        };

                        memory_facts = memory_list.into_iter()
                            .take(15)
                            .map(|m| m.content)
                            .collect();

                        if !memory_facts.is_empty() {
                            memory_facts.reverse(); // oldest first for natural reading
                            info!("[build_prompt] Recency memory fallback: {} memories", memory_facts.len());
                        }
                    }

                    if !memory_facts.is_empty() {
                        let memory_block = format_memory_block(
                            "[Remembered Facts — things you know from past interactions]",
                            &memory_facts,
                        );

                        let memory_message = ChatMessage {
                            role: MessageRole::System,
                            content: memory_block,
                        };

                        // Enforce token cap — truncate if over budget
                        let mem_tokens = count_message_tokens(&memory_message);
                        if mem_tokens <= context_budget.max_memory_tokens {
                            prompt.push(memory_message);
                        } else {
                            // Progressively drop facts until under budget
                            let mut facts = memory_facts;
                            while facts.len() > 1 {
                                facts.pop();
                                let truncated_block = format_memory_block(
                                    "[Remembered Facts — things you know from past interactions]",
                                    &facts,
                                );
                                let truncated_msg = ChatMessage {
                                    role: MessageRole::System,
                                    content: truncated_block,
                                };
                                if count_message_tokens(&truncated_msg) <= context_budget.max_memory_tokens {
                                    prompt.push(truncated_msg);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Inject character emotional state as a dynamic context layer.
            // Placed last in the system prompt so it's closest to the conversation history
            // and carries the most weight in the attention window.
            if is_multi_char {
                // ── Multi-character emotional states ──
                let mut state_parts: Vec<String> = Vec::new();
                for conv_char in &active_conv_chars {
                    let cc_char_id = conv_char.character_id.id.to_raw();
                    if let Ok(Some(state)) = CharacterStateRepo::get(db, &cc_char_id, conversation_id).await {
                        state_parts.push(format!(
                            "  {} — {} (mood:{}/100 trust:{}/100 intensity:{}/100) — {}",
                            conv_char.character_name,
                            state.dominant_emotion,
                            state.mood, state.trust, state.arousal,
                            state.state_summary,
                        ));
                    }
                }
                if !state_parts.is_empty() {
                    prompt.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!(
                            "[Current Emotional States]\n{}\n(Let these emotional states colour each character's response naturally.)",
                            state_parts.join("\n")
                        ),
                    });
                }
            } else if let Ok(Some(state)) = CharacterStateRepo::get(db, char_id, conversation_id).await {
                let state_block = format!(
                    "[Current Emotional State]\n\
                     Dominant emotion: {emotion}\n\
                     Mood: {mood}/100  Trust: {trust}/100  Intensity: {arousal}/100\n\
                     Internal state: {summary}\n\
                     (Let this emotional state colour your response naturally — do not announce or describe it explicitly.)",
                    emotion = state.dominant_emotion,
                    mood = state.mood,
                    trust = state.trust,
                    arousal = state.arousal,
                    summary = state.state_summary,
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: state_block,
                });
            }

            // ── Inject current scene state ──
            // Placed after emotional state, before summary — gives the AI spatial awareness
            // of where the story is happening (location, time, weather, characters present).
            if let Ok(Some(scene)) = SceneStateRepo::get(db, conversation_id).await {
                // Scene extraction is told to use the literal "{{user}}"
                // token for the player character (see scene_extractor.rs) —
                // substitute it the same way character-card text is above,
                // so the model never sees a raw unresolved macro here either.
                let chars_list = if scene.characters_present.is_empty() {
                    "unspecified".to_string()
                } else {
                    scene.characters_present.iter()
                        .map(|c| substitute_user_macro(c, persona_name))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let scene_block = format!(
                    "[Current Scene]\n\
                     Location: {name} — {desc}\n\
                     Time: {time} | Weather: {weather}\n\
                     Present: {chars}\n\
                     Atmosphere: {ambient}\n\
                     (Maintain scene consistency. Describe transitions naturally when the story moves to a new location or time.)",
                    name = scene.location_name,
                    desc = scene.location_description,
                    time = scene.time_period,
                    weather = scene.weather,
                    chars = chars_list,
                    ambient = scene.ambient_details,
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: scene_block,
                });
            }

            // ── Inject rolling summary (if exists) ──
            let summary = SummaryRepo::get(db, conversation_id).await.ok().flatten();
            let summary_tokens = if let Some(ref s) = summary {
                let summary_message = ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "[Story So Far — summary of earlier conversation]\n{}",
                        s.summary_text
                    ),
                };
                let tokens = count_message_tokens(&summary_message);
                prompt.push(summary_message);
                tokens
            } else {
                0
            };

            // ── Apply sliding window to conversation history ──
            // Build the PHI message (if any) so it can be counted in the budget
            let phi_message = post_history_instructions.and_then(|phi| {
                let trimmed = phi.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(ChatMessage {
                        role: MessageRole::System,
                        content: trimmed.to_string(),
                    })
                }
            });

            // Collect fixed layers for budget calculation (everything in prompt so far + PHI)
            let mut fixed_for_budget: Vec<ChatMessage> = prompt.clone();
            if let Some(ref phi) = phi_message {
                fixed_for_budget.push(phi.clone());
            }

            let allocation = context_budget.allocate(&fixed_for_budget);
            let window = apply_sliding_window(&chain, allocation.messages_budget);

            info!(
                "[build_prompt] context: {}/{} tokens, {}/{} messages included, {} evicted",
                allocation.fixed_layers_tokens + window.included_tokens,
                context_budget.max_context_tokens,
                window.included.len(),
                chain.len(),
                window.evicted_count,
            );

            let total_messages = chain.len();
            let included_messages = window.included.len();
            let evicted_messages = window.evicted_count;
            let history_tokens = window.included_tokens;

            prompt.extend(window.included);

            // Vector RAG: retrieve semantically relevant evicted messages
            // Fixes applied:
            // - Dedup: exclude messages already in the sliding window
            // - Budget: cap RAG injection to allocation.rag_budget tokens
            // - Cross-conv: when memory_scope = "character", search all conversations
            let rag_tokens = if window.evicted_count > 0 {
                let last_user_content = chain.iter().rev()
                    .find(|m| m.role == MessageRole::User)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();

                if !last_user_content.is_empty() {
                    // Collect IDs of messages in the sliding window for dedup
                    let include_from = chain.len().saturating_sub(included_messages);
                    let window_msg_ids: Vec<String> = branch[include_from..]
                        .iter()
                        .map(|m| m.id.id.to_raw())
                        .collect();

                    let rag_results = match get_default_llm_provider(db).await {
                        Ok(pc) => match create_rig_provider(&pc) {
                            Ok(provider) => {
                                let embed_model = pc.config
                                    .get("embedding_model")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("text-embedding-3-small");

                                // Choose scope: character-wide or conversation-only
                                let (conv_scope, char_scope) = if memory_scope == "character" {
                                    (None, character_id.as_deref())
                                } else {
                                    (Some(conversation_id as &str), None)
                                };

                                query_relevant_context(
                                    db, &provider, embed_model,
                                    conv_scope, char_scope,
                                    &last_user_content,
                                    5, 0.7, &window_msg_ids,
                                ).await.unwrap_or_default()
                            }
                            Err(_) => vec![],
                        },
                        Err(_) => vec![],
                    };

                    if !rag_results.is_empty() {
                        let rag_text = rag_results.iter()
                            .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        let rag_message = ChatMessage {
                            role: MessageRole::System,
                            content: format!(
                                "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                                rag_text
                            ),
                        };
                        let tokens = count_message_tokens(&rag_message);

                        // Enforce RAG budget cap
                        if tokens <= allocation.rag_budget {
                            prompt.push(rag_message);
                            tokens
                        } else {
                            // Try with fewer results until within budget
                            let mut truncated = rag_results.clone();
                            while truncated.len() > 1 {
                                truncated.pop();
                                let text = truncated.iter()
                                    .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                let msg = ChatMessage {
                                    role: MessageRole::System,
                                    content: format!(
                                        "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                                        text
                                    ),
                                };
                                let t = count_message_tokens(&msg);
                                if t <= allocation.rag_budget {
                                    prompt.push(msg);
                                    break;
                                }
                            }
                            // Return whatever tokens we actually used
                            prompt.last()
                                .map(count_message_tokens)
                                .unwrap_or(0)
                        }
                    } else { 0 }
                } else { 0 }
            } else { 0 };

            // PHI goes last — after history, maximum attention weight
            if let Some(phi) = phi_message {
                prompt.push(phi);
            }

            return Ok((prompt, ContextStats {
                total_budget: context_budget.max_context_tokens,
                fixed_tokens: allocation.fixed_layers_tokens,
                history_tokens,
                summary_tokens,
                total_messages,
                included_messages,
                evicted_messages,
                rag_tokens,
            }));
        }
    } else {
        // No character — just build the message chain
        let branch = MessageRepo::get_branch(db, up_to_message_id).await?;
        let chain: Vec<ChatMessage> = branch.iter().map(|m| {
            ChatMessage {
                role: match m.role {
                    MessageRole::User => MessageRole::User,
                    MessageRole::Assistant => MessageRole::Assistant,
                    MessageRole::System => MessageRole::System,
                },
                content: m.content.clone(),
            }
        }).collect();

        // Conversation-scoped memories even without a character
        if memory_scope != "none" {
            let memory_list = MemoryRepo::list(db, None, Some(conversation_id)).await.unwrap_or_default();
            let memory_rows: Vec<_> = memory_list.into_iter().take(20).collect();
            if !memory_rows.is_empty() {
                let mut facts: Vec<String> = memory_rows.into_iter().map(|m| m.content).collect();
                facts.reverse();
                let memory_block = format_memory_block(
                    "[Remembered Facts — things you know from past interactions]",
                    &facts,
                );
                prompt.push(ChatMessage {
                    role: MessageRole::System,
                    content: memory_block,
                });
            }
        }

        // ── Inject rolling summary (if exists) ──
        let summary = SummaryRepo::get(db, conversation_id).await.ok().flatten();
        let summary_tokens = if let Some(ref s) = summary {
            let summary_message = ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "[Story So Far — summary of earlier conversation]\n{}",
                    s.summary_text
                ),
            };
            let tokens = count_message_tokens(&summary_message);
            prompt.push(summary_message);
            tokens
        } else {
            0
        };

        // ── Apply sliding window (no-character branch) ──
        let phi_message = post_history_instructions.and_then(|phi| {
            let trimmed = phi.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(ChatMessage {
                    role: MessageRole::System,
                    content: trimmed.to_string(),
                })
            }
        });

        let mut fixed_for_budget: Vec<ChatMessage> = prompt.clone();
        if let Some(ref phi) = phi_message {
            fixed_for_budget.push(phi.clone());
        }

        let allocation = context_budget.allocate(&fixed_for_budget);
        let window = apply_sliding_window(&chain, allocation.messages_budget);

        info!(
            "[build_prompt] context: {}/{} tokens, {}/{} messages included, {} evicted",
            allocation.fixed_layers_tokens + window.included_tokens,
            context_budget.max_context_tokens,
            window.included.len(),
            chain.len(),
            window.evicted_count,
        );

        let total_messages = chain.len();
        let included_messages = window.included.len();
        let evicted_messages = window.evicted_count;
        let history_tokens = window.included_tokens;

        prompt.extend(window.included);

        // Vector RAG: retrieve semantically relevant evicted messages (non-character path)
        let rag_tokens = if window.evicted_count > 0 {
            let last_user_content = chain.iter().rev()
                .find(|m| m.role == MessageRole::User)
                .map(|m| m.content.clone())
                .unwrap_or_default();

            if !last_user_content.is_empty() {
                let exclude_ids: Vec<String> = vec![];
                let rag_results = match get_default_llm_provider(db).await {
                    Ok(pc) => match create_rig_provider(&pc) {
                        Ok(provider) => {
                            let embed_model = pc.config
                                .get("embedding_model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("text-embedding-3-small");
                            query_relevant_context(
                                db, &provider, embed_model,
                                Some(conversation_id), None,
                                &last_user_content,
                                5, 0.7, &exclude_ids,
                            ).await.unwrap_or_default()
                        }
                        Err(_) => vec![],
                    },
                    Err(_) => vec![],
                };

                if !rag_results.is_empty() {
                    let rag_text = rag_results.iter()
                        .map(|r| format!("[{:.0}% relevance] {}: {}", r.similarity * 100.0, r.role, r.content))
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    let rag_message = ChatMessage {
                        role: MessageRole::System,
                        content: format!(
                            "[Relevant Past Context — retrieved from earlier conversation history]\n{}",
                            rag_text
                        ),
                    };
                    let tokens = count_message_tokens(&rag_message);
                    if tokens <= allocation.rag_budget {
                        prompt.push(rag_message);
                        tokens
                    } else { 0 }
                } else { 0 }
            } else { 0 }
        } else { 0 };

        if let Some(phi) = phi_message {
            prompt.push(phi);
        }

        Ok((prompt, ContextStats {
            total_budget: context_budget.max_context_tokens,
            fixed_tokens: allocation.fixed_layers_tokens,
            history_tokens,
            summary_tokens,
            total_messages,
            included_messages,
            evicted_messages,
            rag_tokens,
        }))
    }
}
