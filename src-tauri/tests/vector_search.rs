//! Integration test for the vector-similarity half of retrieval — the
//! `HNSW` index (renamed from `MTREE` for the surrealdb 3.x migration) and
//! `vector::similarity::cosine()` query path in `EmbeddingRepo::query_similar`
//! / `query_memory_similar`. `hybrid_search.rs` intentionally only covers
//! the BM25/keyword half since it needs a live embedding provider to
//! generate real vectors — this test sidesteps that by hand-crafting
//! embeddings with a known similarity structure, so it needs no external
//! API and can assert exact ranking.
//!
//! Runs against a real embedded SurrealDB instance (same engine the app
//! uses), so it exercises the actual `DEFINE INDEX ... HNSW ...` statement
//! and `vector::similarity::cosine()` at runtime, not just compile-checks
//! the surrounding Rust.

use janus_lib::db::characters::CharacterRepo;
use janus_lib::db::conversations::ConversationRepo;
use janus_lib::db::embeddings::EmbeddingRepo;
use janus_lib::db::init_database;
use janus_lib::db::messages::MessageRepo;
use janus_lib::db::value_bridge::record_id_to_string;

async fn test_db() -> (
    surrealdb::Surreal<surrealdb::engine::local::Db>,
    std::path::PathBuf,
) {
    let dir = std::env::temp_dir().join(format!("mythic_vec_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir)
        .await
        .expect("init_database should succeed");
    (db, dir)
}

fn cleanup(dir: std::path::PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn vector_similarity_search_ranks_by_actual_cosine_distance() {
    let (db, dir) = test_db().await;

    let character = CharacterRepo::create(&db, "Elara", serde_json::json!({"name": "Elara"}))
        .await
        .expect("create character");
    let conv = ConversationRepo::create(
        &db,
        Some(&record_id_to_string(&character.id)),
        Some("Vector test conversation"),
        None,
    )
    .await
    .expect("create conversation");
    let conv_id = record_id_to_string(&conv.id);

    // Hand-crafted 4-dimensional embeddings with a known similarity
    // structure relative to the query vector [1.0, 0.0, 0.0, 0.0]:
    //   msg_close:  [1.0, 0.0, 0.0, 0.0] -> cosine similarity 1.0 (identical)
    //   msg_near:   [0.9, 0.1, 0.0, 0.0] -> high similarity, but not 1.0
    //   msg_far:    [0.0, 1.0, 0.0, 0.0] -> cosine similarity 0.0 (orthogonal)
    let msg_close = MessageRepo::create(&db, &conv_id, "user", "the exact match", None, None)
        .await
        .expect("create msg_close");
    let msg_near = MessageRepo::create(&db, &conv_id, "user", "a near match", None, None)
        .await
        .expect("create msg_near");
    let msg_far = MessageRepo::create(
        &db,
        &conv_id,
        "user",
        "a totally unrelated message",
        None,
        None,
    )
    .await
    .expect("create msg_far");

    // Ensure the HNSW index exists with the right dimension before storing
    // — mirrors what commands::embeddings does before EmbeddingRepo::store.
    EmbeddingRepo::ensure_vector_index(&db, 4)
        .await
        .expect("ensure_vector_index should define the HNSW index without error");

    EmbeddingRepo::store(
        &db,
        &record_id_to_string(&msg_close.id),
        &conv_id,
        &[1.0, 0.0, 0.0, 0.0],
        "test-model",
        None,
    )
    .await
    .expect("store msg_close embedding");
    EmbeddingRepo::store(
        &db,
        &record_id_to_string(&msg_near.id),
        &conv_id,
        &[0.9, 0.1, 0.0, 0.0],
        "test-model",
        None,
    )
    .await
    .expect("store msg_near embedding");
    EmbeddingRepo::store(
        &db,
        &record_id_to_string(&msg_far.id),
        &conv_id,
        &[0.0, 1.0, 0.0, 0.0],
        "test-model",
        None,
    )
    .await
    .expect("store msg_far embedding");

    let results = EmbeddingRepo::query_similar(
        &db,
        Some(&conv_id),
        None,
        &[1.0, 0.0, 0.0, 0.0],
        10,
        0.0, // min_similarity — include everything, we're asserting on order/values
        &[],
    )
    .await
    .expect("query_similar should not error against the HNSW index");

    assert_eq!(
        results.len(),
        3,
        "expected all 3 stored embeddings to be returned"
    );

    // Ranking must reflect actual cosine distance, not insertion order.
    assert_eq!(results[0].content, "the exact match");
    assert_eq!(results[1].content, "a near match");
    assert_eq!(results[2].content, "a totally unrelated message");

    // Similarity values themselves must be numerically sane, not just
    // ordered — proves vector::similarity::cosine() actually computed a
    // real value through the HNSW-indexed field, not a stub/zero.
    assert!(
        (results[0].similarity - 1.0).abs() < 1e-6,
        "exact match should have cosine similarity ~1.0, got {}",
        results[0].similarity
    );
    assert!(
        results[1].similarity > 0.8 && results[1].similarity < 1.0,
        "near match should have high but non-identical similarity, got {}",
        results[1].similarity
    );
    assert!(
        results[2].similarity.abs() < 1e-6,
        "orthogonal vector should have cosine similarity ~0.0, got {}",
        results[2].similarity
    );

    // min_similarity threshold must actually filter — re-query excluding
    // the orthogonal (0.0-similarity) result.
    let filtered = EmbeddingRepo::query_similar(
        &db,
        Some(&conv_id),
        None,
        &[1.0, 0.0, 0.0, 0.0],
        10,
        0.5,
        &[],
    )
    .await
    .expect("query_similar with threshold should not error");
    assert_eq!(
        filtered.len(),
        2,
        "min_similarity=0.5 should exclude the orthogonal match"
    );

    cleanup(dir);
}

#[tokio::test]
async fn vector_similarity_search_respects_top_k_limit() {
    let (db, dir) = test_db().await;

    let character = CharacterRepo::create(&db, "Elara", serde_json::json!({"name": "Elara"}))
        .await
        .expect("create character");
    let conv = ConversationRepo::create(
        &db,
        Some(&record_id_to_string(&character.id)),
        Some("Top-k test conversation"),
        None,
    )
    .await
    .expect("create conversation");
    let conv_id = record_id_to_string(&conv.id);

    EmbeddingRepo::ensure_vector_index(&db, 2)
        .await
        .expect("ensure_vector_index");

    for i in 0..5 {
        let msg = MessageRepo::create(&db, &conv_id, "user", &format!("message {i}"), None, None)
            .await
            .expect("create message");
        EmbeddingRepo::store(
            &db,
            &record_id_to_string(&msg.id),
            &conv_id,
            &[1.0, 0.0],
            "test-model",
            None,
        )
        .await
        .expect("store embedding");
    }

    let results = EmbeddingRepo::query_similar(&db, Some(&conv_id), None, &[1.0, 0.0], 2, 0.0, &[])
        .await
        .expect("query_similar should not error");
    assert_eq!(results.len(), 2, "top_k=2 should cap the result count");

    cleanup(dir);
}
