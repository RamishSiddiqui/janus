//! The shared stream-completion task: consumes provider stream chunks until
//! the response finishes (or errors), saves the final message, handles
//! multi-character segment attribution, and emits the frontend events.
//! Used by both `send_message` and `retry_failed_message` — see
//! `StreamCompletionCtx`'s doc comment for why this used to be two ~300-line
//! copies maintained in lockstep.

use std::sync::Arc;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tauri::Emitter;
use tracing::{error, info, warn};

use super::pipeline::{spawn_embed_message, spawn_scene_extraction};
use crate::context::prompt_builder::ContextStats;
use crate::context::response_parser::{parse_multi_character_response, resolve_character_id};
use crate::context::summary::generate_rolling_summary;
use crate::db::messages::MessageRepo;
use crate::db::summaries::SummaryRepo;
use crate::models::conversation::{ChatMessage, GenerationParams};
use crate::models::provider::ProviderConfig;
use crate::providers::resolve::{create_rig_provider, get_default_llm_provider};
use crate::providers::traits::StreamChunk;

/// Payload emitted to the frontend via Tauri events during streaming.
#[derive(Clone, serde::Serialize)]
pub(crate) struct StreamEvent {
    /// "delta" | "done" | "error"
    pub(crate) event_type: String,
    /// The text content (delta text, full response, or error message)
    pub(crate) content: String,
    /// The message ID of the assistant response being built
    pub(crate) message_id: String,
}

/// Which caller a stream-completion task is running for — the two copies of
/// this logic (a fresh send vs. a retry of a failed message) are otherwise
/// structurally identical, but differ in the exact log wording used and in
/// whether a rolling-summary pass gets triggered (see `context_stats` on
/// `StreamCompletionCtx`: `None` for a retry, since it never newly evicts
/// anything the original send didn't already account for).
pub(crate) enum StreamOrigin {
    Send,
    Retry,
}

impl StreamOrigin {
    /// Appended to a few multi-char log lines so send vs. retry stay
    /// distinguishable in the logs — purely cosmetic.
    fn log_suffix(&self) -> &'static str {
        match self {
            StreamOrigin::Send => "",
            StreamOrigin::Retry => " (retry)",
        }
    }
}

/// Everything a stream-completion task (the loop that consumes provider
/// stream chunks, saves the final response, handles multi-character
/// attribution, and emits frontend events) needs — bundled so `send_message`
/// and `retry_failed_message` can share one implementation instead of
/// maintaining two ~300-line copies in lockstep.
pub(crate) struct StreamCompletionCtx {
    pub(crate) rx: tokio::sync::mpsc::Receiver<StreamChunk>,
    pub(crate) app: tauri::AppHandle,
    pub(crate) partial: Arc<std::sync::Mutex<String>>,
    pub(crate) reasoning_acc: Arc<std::sync::Mutex<String>>,
    pub(crate) assist_id: String,
    pub(crate) db_for_save: Surreal<Db>,
    pub(crate) conv_id: String,
    pub(crate) active_gens_cleanup:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, crate::GenerationHandle>>>,
    pub(crate) conv_id_cleanup: String,
    pub(crate) stream_char_id: Option<String>,
    pub(crate) stream_mc_names: Vec<String>,
    pub(crate) stream_mc_pairs: Vec<(String, String)>,
    pub(crate) stream_user_msg_id: String,
    pub(crate) stream_user_content: String,
    /// Rebuilding a fresh provider/messages/images for the one-time
    /// transparent retry-on-empty-response path (see inside the function).
    pub(crate) retry_provider_config: ProviderConfig,
    pub(crate) retry_model_id: String,
    pub(crate) retry_gen_params: GenerationParams,
    pub(crate) retry_messages: Vec<ChatMessage>,
    pub(crate) retry_images: Vec<(Vec<u8>, String)>,
    /// `Some` only for the original send — see `StreamOrigin` doc comment.
    pub(crate) context_stats: Option<ContextStats>,
    pub(crate) origin: StreamOrigin,
}

/// Consumes provider stream chunks until the response completes (or errors),
/// saving the final message, handling multi-character segment attribution,
/// and emitting the frontend events — shared by `send_message` and
/// `retry_failed_message`. Spawned as its own task by both callers; never
/// awaited, so it must not return anything meaningful.
pub(crate) async fn run_stream_completion(mut ctx: StreamCompletionCtx) {
    let (log_tag, save_err_msg, completed_msg) = match ctx.origin {
        StreamOrigin::Send => (
            "[send_message]",
            "Failed to save response",
            "Chat response completed",
        ),
        StreamOrigin::Retry => (
            "[retry_failed_message]",
            "Failed to save retry response",
            "Retry response completed",
        ),
    };
    let suffix = ctx.origin.log_suffix();

    let app = ctx.app;
    let db_for_save = ctx.db_for_save;
    let conv_id = ctx.conv_id;
    let assist_id = ctx.assist_id;
    let partial = ctx.partial;
    let reasoning_acc = ctx.reasoning_acc;
    let active_gens_cleanup = ctx.active_gens_cleanup;
    let conv_id_cleanup = ctx.conv_id_cleanup;
    let stream_char_id = ctx.stream_char_id;
    let stream_mc_names = ctx.stream_mc_names;
    let stream_mc_pairs = ctx.stream_mc_pairs;
    let stream_user_msg_id = ctx.stream_user_msg_id;
    let stream_user_content = ctx.stream_user_content;
    let retry_provider_config = ctx.retry_provider_config;
    let retry_model_id = ctx.retry_model_id;
    let retry_gen_params = ctx.retry_gen_params;
    let retry_messages = ctx.retry_messages;
    let retry_images = ctx.retry_images;

    let mut attempted_retry = false;
    while let Some(chunk) = ctx.rx.recv().await {
        match chunk {
            StreamChunk::Delta(text) => {
                if let Ok(mut p) = partial.lock() {
                    p.push_str(&text);
                }
                let _ = app.emit(
                    "chat-stream",
                    StreamEvent {
                        event_type: "delta".to_string(),
                        content: text,
                        message_id: assist_id.clone(),
                    },
                );
            }
            StreamChunk::ReasoningDelta(text) => {
                if let Ok(mut r) = reasoning_acc.lock() {
                    r.push_str(&text);
                }
                let _ = app.emit(
                    "chat-stream",
                    StreamEvent {
                        event_type: "reasoning".to_string(),
                        content: text,
                        message_id: assist_id.clone(),
                    },
                );
            }
            StreamChunk::Done(full_text) => {
                // Some providers fail "quietly" under load — the stream
                // completes with zero content instead of a hard error
                // (observed: OpenRouter-routed free models returning a
                // 502 "worker request limit reached" that never
                // surfaces as StreamChunk::Error). Retry once,
                // transparently, before treating this as a real
                // failure — exactly what a manual "Regenerate" click
                // already does, just automatic.
                if full_text.trim().is_empty() && !attempted_retry {
                    attempted_retry = true;
                    warn!(
                        "{} Empty response for conversation {} — retrying once (likely a transient upstream overload)",
                        log_tag, conv_id
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    if let Ok(retry_provider) = create_rig_provider(&retry_provider_config) {
                        let (tx2, rx2) = tokio::sync::mpsc::channel::<StreamChunk>(64);
                        let rm = retry_model_id.clone();
                        let rmsgs = retry_messages.clone();
                        let rgp = retry_gen_params.clone();
                        let rimgs = retry_images.clone();
                        let tx2c = tx2.clone();
                        let retry_task = tokio::spawn(async move {
                            if let Err(e) = retry_provider
                                .generate_stream(&rm, &rmsgs, &rimgs, &rgp, tx2c.clone())
                                .await
                            {
                                let _ = tx2c
                                    .send(StreamChunk::Error(format!("Retry stream failed: {}", e)))
                                    .await;
                            }
                        });
                        // Re-point the single-flight lock at the retry task — the
                        // original task has already finished (it's what produced
                        // the empty response being retried), so Stop must abort
                        // THIS task now. Without this, cancel_generation aborts an
                        // already-dead handle (a no-op) and the retry keeps running
                        // ungoverned after the user believed they'd cancelled.
                        active_gens_cleanup.lock().await.insert(
                            conv_id_cleanup.clone(),
                            crate::GenerationHandle {
                                abort: retry_task.abort_handle(),
                                partial: Some(partial.clone()),
                                assistant_message_id: assist_id.clone(),
                            },
                        );
                        ctx.rx = rx2;
                        continue;
                    }
                    // Couldn't even rebuild the provider — fall through
                    // and treat this as the final (failed) result.
                }

                // Save the complete response to the database
                if let Err(e) = MessageRepo::update(&db_for_save, &assist_id, &full_text).await {
                    error!("{}: {}", save_err_msg, e);
                }
                let reasoning_final = reasoning_acc.lock().map(|r| r.clone()).unwrap_or_default();
                if !reasoning_final.is_empty() {
                    if let Err(e) =
                        MessageRepo::set_reasoning(&db_for_save, &assist_id, &reasoning_final).await
                    {
                        warn!("Failed to save reasoning trace: {}", e);
                    }
                }

                // ── Multi-character response parsing ──
                // Parsing is now always attempted (not gated on the conversation
                // already having 2+ registered characters) — a solo conversation
                // can legitimately introduce a brand-new speaker's [Name]: marker
                // (see the "Other Characters Present" prompt addition in
                // `context::prompt_builder`). The `is_ordinary_turn` check below
                // keeps the common case (no marker at all, entire reply is the
                // primary character) a true no-op — zero extra DB writes.
                let mut multi_char_handled = false;
                let fallback = stream_mc_names
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Character".to_string());

                let segments =
                    parse_multi_character_response(&full_text, &stream_mc_names, &fallback);
                let is_ordinary_turn =
                    segments.len() == 1 && segments[0].character_name == fallback;

                if !is_ordinary_turn {
                    if segments.len() > 1 {
                        info!(
                            "[multi-char] Parsed {} character segments{}",
                            segments.len(),
                            suffix
                        );

                        // Delete the combined parent message — it will be replaced
                        // by individual per-character messages chained sequentially.
                        if let Err(e) = MessageRepo::delete(&db_for_save, &assist_id).await {
                            warn!(
                                "[multi-char] Failed to delete combined parent message {}: {}",
                                assist_id, e
                            );
                        }

                        // Create individual character messages in a chain:
                        // user_msg → segment[0] → segment[1] → … → segment[N]
                        let mut prev_parent = stream_user_msg_id.clone();
                        // Mutable copy — a brand-new speaker registered mid-loop gets
                        // pushed in immediately, so a second appearance of the same
                        // new name later in this SAME response resolves against her
                        // just-created row instead of registering a duplicate.
                        let mut resolution_pairs = stream_mc_pairs.clone();
                        // Index-aligned with `segments` — filled in as each
                        // segment's real row gets created, so the emitted
                        // event below can carry real IDs/character_ids
                        // instead of leaving the frontend to guess at
                        // synthetic `parentId__segN` placeholders (needed
                        // both for the emotional-snapshot follow-up write
                        // and so avatars resolve without a reload).
                        let mut created_segments: Vec<serde_json::Value> =
                            Vec::with_capacity(segments.len());
                        for segment in &segments {
                            // Resolve character ID — synchronously register a brand-new
                            // speaker the LLM voiced but who isn't in the cast yet (e.g. a
                            // solo conversation's [Lena]: line); only fall back to the
                            // primary character if that registration itself fails.
                            let (char_id, full_name) = if let Some(cid) =
                                resolve_character_id(&segment.character_name, &resolution_pairs)
                            {
                                let name = resolution_pairs
                                    .iter()
                                    .find(|(_, id)| *id == cid)
                                    .map(|(name, _)| name.clone())
                                    .unwrap_or_else(|| segment.character_name.clone());
                                (cid, name)
                            } else if let Some((new_id, new_name)) =
                                crate::context::npc::pipeline::register_transient_speaker(
                                    &db_for_save,
                                    &app,
                                    &conv_id,
                                    &segment.character_name,
                                )
                                .await
                            {
                                resolution_pairs.push((new_name.clone(), new_id.clone()));
                                (new_id, new_name)
                            } else {
                                // Fallback: attribute to primary character
                                warn!("[multi-char] Unrecognized character '{}', attributing to primary{}", segment.character_name, suffix);
                                let fallback_id = resolution_pairs
                                    .first()
                                    .map(|(_, id)| id.clone())
                                    .unwrap_or_default();
                                let fallback_name = resolution_pairs
                                    .first()
                                    .map(|(name, _)| name.clone())
                                    .unwrap_or_else(|| segment.character_name.clone());
                                (fallback_id, fallback_name)
                            };

                            match MessageRepo::create_with_character(
                                &db_for_save,
                                &conv_id,
                                "assistant",
                                &segment.content,
                                Some(&prev_parent),
                                &char_id,
                                &full_name,
                            )
                            .await
                            {
                                Ok(created) => {
                                    let segment_id = created.id.id.to_raw();
                                    prev_parent = segment_id.clone();
                                    created_segments.push(serde_json::json!({
                                        "character_name": full_name,
                                        "content": segment.content,
                                        "index": segment.index,
                                        "id": segment_id,
                                        "character_id": char_id,
                                    }));
                                    // The combined parent message (embedded below only
                                    // when multi_char_handled stays false) was deleted
                                    // above — these per-segment rows are the only
                                    // record of this turn's content now, so each needs
                                    // its own embed call or it never gets indexed at all.
                                    spawn_embed_message(
                                        db_for_save.clone(),
                                        app.clone(),
                                        segment_id,
                                        conv_id.clone(),
                                        segment.content.clone(),
                                        Some(char_id.clone()),
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "[multi-char] Failed to create segment for {}: {}",
                                        full_name, e
                                    );
                                }
                            }
                        }

                        multi_char_handled = true;

                        // Emit multi-char event for frontend rendering. Carries the
                        // real created message IDs (for the emotional-snapshot
                        // follow-up write) and the raw full_text/user message (the
                        // "done" event's content is intentionally emptied below to
                        // avoid a duplicate combined-message re-render, so this is
                        // the only place the frontend's emotion pipeline can get them
                        // for a multi-speaker turn).
                        let _ = app.emit(
                            "multi-char-response",
                            serde_json::json!({
                                "conversation_id": conv_id,
                                "segments": created_segments,
                                "parent_message_id": assist_id,
                                "full_text": full_text.clone(),
                                "user_message": stream_user_content.clone(),
                            }),
                        );
                    } else if segments.len() == 1 {
                        // Single character responded — still need to strip the
                        // [CharName]: prefix and set character attribution on the
                        // existing message so the UI shows the name badge properly.
                        // This also covers the entire reply being voiced by one
                        // brand-new speaker (e.g. only [Lena]: — nothing from the
                        // primary at all): without the synchronous registration
                        // fallback here too, resolve_character_id would fail and
                        // this branch would silently do nothing, leaving the raw
                        // unstripped marker sitting in the primary's own bubble.
                        let seg = &segments[0];
                        let resolved =
                            match resolve_character_id(&seg.character_name, &stream_mc_pairs) {
                                Some(cid) => Some((cid, seg.character_name.clone())),
                                None => {
                                    crate::context::npc::pipeline::register_transient_speaker(
                                        &db_for_save,
                                        &app,
                                        &conv_id,
                                        &seg.character_name,
                                    )
                                    .await
                                }
                            };
                        if let Some((char_id, char_name)) = resolved {
                            info!(
                                "[multi-char] Single segment by {}, updating in-place{}",
                                char_name, suffix
                            );
                            if let Err(e) = db_for_save.query(
                                "UPDATE type::thing('messages', $id) SET content = $content, character_id = type::thing('characters', $char_id), character_name = $char_name"
                            )
                                .bind(("id", assist_id.clone()))
                                .bind(("content", seg.content.clone()))
                                .bind(("char_id", char_id.clone()))
                                .bind(("char_name", char_name))
                                .await
                            {
                                warn!("[multi-char] Failed to apply single-segment attribution to message {}{}: {}", assist_id, suffix, e);
                            }

                            // The row now holds the marker-stripped seg.content, not
                            // the raw full_text the trailing embed call below would
                            // otherwise use (and that call is skipped entirely once
                            // multi_char_handled is true) — embed the actual saved
                            // text here instead.
                            spawn_embed_message(
                                db_for_save.clone(),
                                app.clone(),
                                assist_id.clone(),
                                conv_id.clone(),
                                seg.content.clone(),
                                Some(char_id.clone()),
                            );

                            multi_char_handled = true;

                            // Emit single-segment event so the frontend updates the live
                            // message — same enrichment as the multi-segment branch above
                            // (real id/character_id, full_text, user_message) since this
                            // path also empties the "done" event's content below.
                            let _ = app.emit(
                                "multi-char-response",
                                serde_json::json!({
                                    "conversation_id": conv_id,
                                    "segments": [serde_json::json!({
                                        "character_name": seg.character_name,
                                        "content": seg.content,
                                        "index": seg.index,
                                        "id": assist_id,
                                        "character_id": char_id,
                                    })],
                                    "parent_message_id": assist_id,
                                    "full_text": full_text.clone(),
                                    "user_message": stream_user_content.clone(),
                                }),
                            );
                        }
                    }
                }

                // Background: embed assistant message for vector RAG. Only when the
                // multi-char branches above didn't already handle it themselves —
                // the segments.len() > 1 branch deletes `assist_id` outright (its
                // replacement segment rows embed themselves individually above), and
                // the segments.len() == 1 non-primary-speaker branch already embedded
                // the marker-stripped content it actually saved. Embedding here too
                // would either target a message_id that no longer exists (an orphan
                // `message_embeddings` row that inflates the index's "embedded" count
                // without indexing anything real) or duplicate an embedding that was
                // just created against the correct content.
                if !multi_char_handled {
                    spawn_embed_message(
                        db_for_save.clone(),
                        app.clone(),
                        assist_id.clone(),
                        conv_id.clone(),
                        full_text.clone(),
                        stream_char_id.clone(),
                    );
                }

                // Background: extract and update scene state from the AI response.
                // (spawn_scene_extraction also triggers NPC detection internally.)
                spawn_scene_extraction(
                    db_for_save.clone(),
                    app.clone(),
                    conv_id.clone(),
                    assist_id.clone(),
                    full_text.clone(),
                );

                // When multi-char segments were processed, emit done
                // with empty content — the multi-char-response event
                // already handled rendering. Sending full_text here would
                // cause the frontend done handler to re-create the combined
                // message and show duplicates.
                let done_content = if multi_char_handled {
                    String::new()
                } else {
                    full_text
                };
                let _ = app.emit(
                    "chat-stream",
                    StreamEvent {
                        event_type: "done".to_string(),
                        content: done_content,
                        message_id: assist_id.clone(),
                    },
                );

                info!("{} for conversation {}", completed_msg, conv_id);

                // Trigger background summary generation if messages were evicted —
                // only for the original send (see `StreamOrigin` doc comment).
                if let Some(context_stats) = &ctx.context_stats {
                    if context_stats.evicted_messages > 0 {
                        let db_summary = db_for_save.clone();
                        let conv_summary = conv_id.clone();
                        let assist_summary = assist_id.clone();
                        let evicted_n = context_stats.evicted_messages;

                        tokio::spawn(async move {
                            // Debounce: only summarize if >= 10 new evictions since last summary
                            let existing = SummaryRepo::get(&db_summary, &conv_summary)
                                .await
                                .ok()
                                .flatten();
                            let prev_covered = existing
                                .as_ref()
                                .map(|s| s.covered_message_count)
                                .unwrap_or(0);

                            if evicted_n as u32 > prev_covered
                                && (evicted_n as u32 - prev_covered) < 10
                                && existing.is_some()
                            {
                                return; // Not enough new evictions yet
                            }

                            // Re-fetch the full branch to get evicted messages
                            let branch =
                                match MessageRepo::get_branch(&db_summary, &assist_summary).await {
                                    Ok(b) => b,
                                    Err(e) => {
                                        warn!(
                                        "[summary] Failed to fetch branch for conversation {}: {}",
                                        conv_summary, e
                                    );
                                        return;
                                    }
                                };
                            if branch.len() <= evicted_n {
                                return;
                            }

                            let evicted: Vec<ChatMessage> = branch[..evicted_n]
                                .iter()
                                .map(|m| ChatMessage {
                                    role: m.role.clone(),
                                    content: m.content.clone(),
                                })
                                .collect();

                            let provider_config = match get_default_llm_provider(&db_summary).await
                            {
                                Ok(pc) => pc,
                                Err(e) => {
                                    warn!("[summary] No LLM provider available for conversation {}: {}", conv_summary, e);
                                    return;
                                }
                            };
                            let provider = match create_rig_provider(&provider_config) {
                                Ok(p) => p,
                                Err(e) => {
                                    warn!("[summary] Failed to create provider for conversation {}: {}", conv_summary, e);
                                    return;
                                }
                            };

                            let model = provider_config
                                .config
                                .get("model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("default")
                                .to_string();
                            let window_start_id = branch.get(evicted_n).map(|m| m.id.id.to_raw());

                            if let Err(e) = generate_rolling_summary(
                                &db_summary,
                                &provider,
                                &model,
                                &conv_summary,
                                &evicted,
                                existing.as_ref().map(|s| s.summary_text.as_str()),
                                window_start_id.as_deref(),
                            )
                            .await
                            {
                                warn!("[summary] Failed to generate rolling summary for conversation {}: {}", conv_summary, e);
                            }
                        });
                    }
                }

                active_gens_cleanup.lock().await.remove(&conv_id_cleanup);
                break;
            }
            StreamChunk::Error(err) => {
                let _ = app.emit(
                    "chat-stream",
                    StreamEvent {
                        event_type: "error".to_string(),
                        content: err,
                        message_id: assist_id.clone(),
                    },
                );
                active_gens_cleanup.lock().await.remove(&conv_id_cleanup);
                break;
            }
        }
    }
}
