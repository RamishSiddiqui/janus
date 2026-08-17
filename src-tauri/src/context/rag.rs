//! Hybrid RAG (Retrieval-Augmented Generation) for conversation context.
//!
//! Retrieval fuses two signals — vector cosine similarity and BM25 keyword
//! search — via Reciprocal Rank Fusion (RRF) rather than either alone. The
//! two scores live on incompatible scales (cosine is bounded [-1,1], BM25 is
//! an unbounded corpus-relative score), so RRF combines them by *rank*
//! instead of averaging raw scores, which is the standard approach for
//! hybrid search in production RAG systems.
//!
//! Five retrieval paths:
//! - `embed_and_store()` — embeds a chat message (background, after save)
//! - `embed_memory()` — embeds a memory fact (background, after extraction)
//! - `query_relevant_context()` — hybrid-retrieves relevant messages
//! - `query_relevant_memories()` — hybrid-retrieves relevant memory facts
//! - `fuse_rrf()` — the shared rank-fusion step used by both

use std::collections::HashMap;

use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::{debug, info, warn};

use crate::db::embeddings::{EmbeddingRepo, RetrievedContext, RetrievedMemoryContext};
use crate::error::MythicError;
use crate::providers::unified::RigProvider;

/// Standard RRF damping constant — large enough that a single ranker's #1
/// result doesn't dominate the fused score, small enough that rank position
/// still matters. 60 is the widely-used default from the original RRF paper.
const RRF_K: f64 = 60.0;

/// Fuses two ranked ID lists into one score-per-ID map via Reciprocal Rank
/// Fusion: `score(id) += 1 / (k + rank)` for each list the ID appears in
/// (1-indexed rank). An ID present in both lists — the strongest signal —
/// accumulates a contribution from each.
fn fuse_rrf<'a>(ranked_id_lists: &[Vec<&'a str>]) -> HashMap<&'a str, f64> {
    let mut scores: HashMap<&str, f64> = HashMap::new();
    for ids in ranked_id_lists {
        for (rank, id) in ids.iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }
    scores
}

/// Multiplier applied on top of a memory's fused relevance score — tiered
/// memory's contribution to ranking, independent of semantic/keyword match.
///
/// - **Importance** (1-10, default 5/neutral): a manually-set tier that
///   shifts ranking up to ±30% either direction.
/// - **Recency of last access**: memories retrieved recently get a mild,
///   decaying boost (self-reinforcing — frequently-relevant memories become
///   more likely to surface again). Never-yet-accessed memories are
///   recency-neutral rather than penalized, since "never retrieved" just
///   means "new", not "irrelevant".
fn tiered_multiplier(importance: i32, last_accessed: &Option<String>) -> f64 {
    let importance_factor = 1.0 + ((importance as f64 - 5.0) / 5.0) * 0.3;

    let recency_factor = last_accessed
        .as_deref()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| {
            let age_days = (chrono::Utc::now() - dt.with_timezone(&chrono::Utc))
                .num_seconds()
                .max(0) as f64
                / 86400.0;
            1.0 + 0.2 / (1.0 + age_days / 30.0)
        })
        .unwrap_or(1.0);

    importance_factor * recency_factor
}

/// Embeds a chat message and stores the embedding in the database.
/// Designed to be called in a background `tokio::spawn` — failures are
/// logged but never propagated (the chat continues without RAG).
pub async fn embed_and_store(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    message_id: &str,
    conversation_id: &str,
    content: &str,
    character_id: Option<&str>,
) -> Result<(), MythicError> {
    // Skip empty content
    if content.trim().is_empty() {
        return Ok(());
    }

    // Skip if already embedded
    match EmbeddingRepo::exists(db, message_id).await {
        Ok(true) => {
            debug!("[rag] Embedding already exists for message {}", message_id);
            return Ok(());
        }
        Err(e) => {
            warn!("[rag] Failed to check embedding existence: {}", e);
            // Continue anyway — worst case we get a duplicate key error
        }
        _ => {}
    }

    // Generate embedding
    let embeddings = provider
        .generate_embedding(embedding_model, vec![content.to_string()])
        .await?;

    let embedding = embeddings
        .into_iter()
        .next()
        .ok_or_else(|| MythicError::Provider("Embedding API returned empty result".to_string()))?;

    // Store with character_id for cross-conversation search
    EmbeddingRepo::store(
        db,
        message_id,
        conversation_id,
        &embedding,
        embedding_model,
        character_id,
    )
    .await?;

    info!(
        "[rag] Embedded message {} ({} dimensions)",
        message_id,
        embedding.len()
    );

    Ok(())
}

/// Embeds a memory fact and stores the embedding in the vector database.
/// This enables semantic retrieval of memories during prompt building,
/// replacing the recency-ordered list with relevance-ordered retrieval.
pub async fn embed_memory(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    memory_id: &str,
    character_id: &str,
    content: &str,
) -> Result<(), MythicError> {
    if content.trim().is_empty() {
        return Ok(());
    }

    // Skip if already embedded
    match EmbeddingRepo::memory_exists(db, memory_id).await {
        Ok(true) => {
            debug!("[rag] Embedding already exists for memory {}", memory_id);
            return Ok(());
        }
        Err(e) => {
            warn!("[rag] Failed to check memory embedding existence: {}", e);
        }
        _ => {}
    }

    // Generate embedding
    let embeddings = provider
        .generate_embedding(embedding_model, vec![content.to_string()])
        .await?;

    let embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result for memory".to_string())
    })?;

    // Store as memory entry
    EmbeddingRepo::store_memory(db, memory_id, character_id, &embedding, embedding_model).await?;

    info!(
        "[rag] Embedded memory {} ({} dimensions)",
        memory_id,
        embedding.len()
    );

    Ok(())
}

/// Retrieves messages relevant to the query text via hybrid search: vector
/// cosine similarity (semantic) fused with BM25 keyword search (exact-term)
/// through Reciprocal Rank Fusion. Used during `build_prompt()` to inject
/// relevant older messages into context.
///
/// Supports two scopes:
/// - **Conversation-scoped**: `conversation_id = Some(...)` — searches within one conversation
/// - **Character-scoped**: `character_id = Some(...)` — searches across all conversations
pub async fn query_relevant_context(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    conversation_id: Option<&str>,
    character_id: Option<&str>,
    query_text: &str,
    top_k: usize,
    min_similarity: f64,
    exclude_message_ids: &[String],
) -> Result<Vec<RetrievedContext>, MythicError> {
    if query_text.trim().is_empty() {
        return Ok(vec![]);
    }

    // Embed the query
    let embeddings = provider
        .generate_embedding(embedding_model, vec![query_text.to_string()])
        .await?;

    let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result for query".to_string())
    })?;

    // Pull a wider candidate pool than top_k from each ranker so RRF has
    // enough overlap between the two lists to actually rerank.
    let candidate_k = top_k * 3;

    let vector_hits = EmbeddingRepo::query_similar(
        db,
        conversation_id,
        character_id,
        &query_embedding,
        candidate_k,
        min_similarity,
        exclude_message_ids,
    )
    .await?;

    // Keyword search is best-effort — if the FTS index or query fails for
    // any reason, hybrid retrieval degrades gracefully to vector-only rather
    // than failing the whole RAG step.
    let keyword_hits = EmbeddingRepo::keyword_search_messages(
        db,
        conversation_id,
        character_id,
        query_text,
        candidate_k,
        exclude_message_ids,
    )
    .await
    .unwrap_or_else(|e| {
        warn!(
            "[rag] Keyword search failed, falling back to vector-only: {}",
            e
        );
        vec![]
    });

    let vector_ids: Vec<&str> = vector_hits.iter().map(|h| h.message_id.as_str()).collect();
    let keyword_ids: Vec<&str> = keyword_hits.iter().map(|h| h.message_id.as_str()).collect();
    let fused_scores = fuse_rrf(&[vector_ids, keyword_ids]);

    let mut ranked_ids: Vec<&str> = fused_scores.keys().copied().collect();
    ranked_ids.sort_by(|a, b| {
        fused_scores[b]
            .partial_cmp(&fused_scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let max_score = fused_scores
        .values()
        .cloned()
        .fold(0.0_f64, f64::max)
        .max(f64::EPSILON);

    let by_id: HashMap<&str, &RetrievedContext> = vector_hits
        .iter()
        .chain(keyword_hits.iter())
        .map(|h| (h.message_id.as_str(), h))
        .collect();

    let results: Vec<RetrievedContext> = ranked_ids
        .into_iter()
        .take(top_k)
        .filter_map(|id| {
            by_id.get(id).map(|hit| RetrievedContext {
                message_id: id.to_string(),
                role: hit.role.clone(),
                content: hit.content.clone(),
                similarity: fused_scores[id] / max_score,
            })
        })
        .collect();

    if !results.is_empty() {
        info!(
            "[rag] Hybrid retrieval: {} vector + {} keyword candidates fused to {} results",
            vector_hits.len(),
            keyword_hits.len(),
            results.len(),
        );
    }

    Ok(results)
}

/// Retrieves memory facts relevant to the query text via the same hybrid
/// (vector + BM25, RRF-fused) approach as `query_relevant_context`. Used
/// during `build_prompt()` to replace the recency-ordered memory list with
/// relevance-ordered retrieval.
pub async fn query_relevant_memories(
    db: &Surreal<Db>,
    provider: &RigProvider,
    embedding_model: &str,
    character_id: &str,
    query_text: &str,
    top_k: usize,
    min_similarity: f64,
    conversation_id: Option<&str>,
) -> Result<Vec<RetrievedMemoryContext>, MythicError> {
    if query_text.trim().is_empty() {
        return Ok(vec![]);
    }

    // Embed the query
    let embeddings = provider
        .generate_embedding(embedding_model, vec![query_text.to_string()])
        .await?;

    let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
        MythicError::Provider("Embedding API returned empty result for memory query".to_string())
    })?;

    let candidate_k = top_k * 3;

    let vector_hits = EmbeddingRepo::query_memory_similar(
        db,
        character_id,
        &query_embedding,
        candidate_k,
        min_similarity,
        conversation_id,
    )
    .await?;

    let keyword_hits = EmbeddingRepo::keyword_search_memories(
        db,
        character_id,
        query_text,
        candidate_k,
        conversation_id,
    )
    .await
    .unwrap_or_else(|e| {
        warn!(
            "[rag] Keyword memory search failed, falling back to vector-only: {}",
            e
        );
        vec![]
    });

    let vector_ids: Vec<&str> = vector_hits.iter().map(|h| h.memory_id.as_str()).collect();
    let keyword_ids: Vec<&str> = keyword_hits.iter().map(|h| h.memory_id.as_str()).collect();
    let fused_scores = fuse_rrf(&[vector_ids, keyword_ids]);

    let by_id: HashMap<&str, &RetrievedMemoryContext> = vector_hits
        .iter()
        .chain(keyword_hits.iter())
        .map(|h| (h.memory_id.as_str(), h))
        .collect();

    // Apply the tiered (importance + recency) multiplier on top of the fused
    // rank score, then re-sort — this is the actual "tiered memory" ranking
    // step, not just semantic/keyword relevance.
    let mut tiered_scores: HashMap<&str, f64> = HashMap::with_capacity(fused_scores.len());
    for (&id, &score) in fused_scores.iter() {
        if let Some(hit) = by_id.get(id) {
            tiered_scores.insert(
                id,
                score * tiered_multiplier(hit.importance, &hit.last_accessed),
            );
        }
    }

    let mut ranked_ids: Vec<&str> = tiered_scores.keys().copied().collect();
    ranked_ids.sort_by(|a, b| {
        tiered_scores[b]
            .partial_cmp(&tiered_scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let max_score = tiered_scores
        .values()
        .cloned()
        .fold(0.0_f64, f64::max)
        .max(f64::EPSILON);

    let results: Vec<RetrievedMemoryContext> = ranked_ids
        .into_iter()
        .take(top_k)
        .filter_map(|id| {
            by_id.get(id).map(|hit| RetrievedMemoryContext {
                memory_id: id.to_string(),
                content: hit.content.clone(),
                is_canon: hit.is_canon,
                similarity: tiered_scores[id] / max_score,
                importance: hit.importance,
                last_accessed: hit.last_accessed.clone(),
            })
        })
        .collect();

    if !results.is_empty() {
        info!(
            "[rag] Hybrid retrieval: {} vector + {} keyword memory candidates fused to {} results",
            vector_hits.len(),
            keyword_hits.len(),
            results.len(),
        );

        // Best-effort: bump access tracking for every memory actually
        // surfaced, so tiering self-reinforces over time. Never blocks or
        // fails retrieval — this is bookkeeping, not the critical path.
        let db_touch = db.clone();
        let ids: Vec<String> = results.iter().map(|r| r.memory_id.clone()).collect();
        tokio::spawn(async move {
            for id in ids {
                if let Err(e) = crate::db::memories::MemoryRepo::bump_access(&db_touch, &id).await {
                    warn!("[rag] Failed to bump access for memory {}: {}", id, e);
                }
            }
        });
    }

    Ok(results)
}
