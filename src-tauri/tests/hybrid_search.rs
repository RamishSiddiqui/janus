//! Integration test for the hybrid (BM25 + vector) retrieval query layer
//! added for RRF-based RAG. Runs against a real embedded SurrealDB instance
//! (same engine the app uses) to exercise the actual SurrealQL — the `@1@`
//! match operator and `search::score()` — rather than just compiling it.
//!
//! This test caught a real runtime-only bug: SurrealDB requires `ORDER BY`
//! to reference an *aliased* `search::score()` column from the SELECT list
//! (`search::score(1) AS relevance` ... `ORDER BY relevance`), not call
//! `search::score()` directly in the ORDER BY clause. The latter type-checks
//! and compiles fine but fails at query-execution time with "Missing order
//! idiom `search` in statement selection" — invisible to `cargo check`.
//!
//! This intentionally only covers `EmbeddingRepo::keyword_search_messages`
//! and `keyword_search_memories` (the new BM25 query paths), since the
//! vector-similarity half requires a live embedding provider and is
//! exercised indirectly through those code paths already.

use janus_lib::db::characters::CharacterRepo;
use janus_lib::db::conversations::ConversationRepo;
use janus_lib::db::embeddings::EmbeddingRepo;
use janus_lib::db::init_database;
use janus_lib::db::memories::MemoryRepo;
use janus_lib::db::messages::MessageRepo;

async fn test_db() -> (
    surrealdb::Surreal<surrealdb::engine::local::Db>,
    std::path::PathBuf,
) {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir)
        .await
        .expect("init_database should succeed");
    (db, dir)
}

fn cleanup(dir: std::path::PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn keyword_search_messages_finds_exact_term_and_respects_scope() {
    let (db, dir) = test_db().await;

    let character = CharacterRepo::create(&db, "Elara", serde_json::json!({"name": "Elara"}))
        .await
        .expect("create character");

    let conv_a = ConversationRepo::create(
        &db,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &character.id,
        )),
        Some("Conv A"),
        None,
    )
    .await
    .expect("create conversation A");
    let conv_b = ConversationRepo::create(
        &db,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &character.id,
        )),
        Some("Conv B"),
        None,
    )
    .await
    .expect("create conversation B");

    MessageRepo::create(
        &db,
        &janus_lib::db::value_bridge::record_id_to_string(&conv_a.id),
        "user",
        "I saw a griffin over the mountains",
        None,
        None,
    )
    .await
    .expect("create message with distinctive term");
    MessageRepo::create(
        &db,
        &janus_lib::db::value_bridge::record_id_to_string(&conv_a.id),
        "assistant",
        "That sounds like an ordinary hawk to me",
        None,
        None,
    )
    .await
    .expect("create unrelated message");
    let griffin_b = MessageRepo::create(
        &db,
        &janus_lib::db::value_bridge::record_id_to_string(&conv_b.id),
        "user",
        "Another griffin sighting near the tower",
        None,
        None,
    )
    .await
    .expect("create message in conversation B");

    // Conversation-scoped: only conv_a's griffin message should come back.
    let hits_a = EmbeddingRepo::keyword_search_messages(
        &db,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &conv_a.id,
        )),
        None,
        "griffin",
        10,
        &[],
    )
    .await
    .expect("keyword search in conv_a should not error");
    assert_eq!(
        hits_a.len(),
        1,
        "expected exactly one griffin match scoped to conv_a"
    );
    assert!(hits_a[0].content.contains("griffin"));

    // The unrelated message in the same conversation must not match.
    let hits_hawk = EmbeddingRepo::keyword_search_messages(
        &db,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &conv_a.id,
        )),
        None,
        "hawk",
        10,
        &[],
    )
    .await
    .expect("keyword search for hawk should not error");
    assert_eq!(hits_hawk.len(), 1);
    assert!(hits_hawk[0].content.contains("hawk"));

    // Character-scoped (no conversation filter): both conversations' griffin
    // messages should be found since they share the same character.
    let hits_char = EmbeddingRepo::keyword_search_messages(
        &db,
        None,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &character.id,
        )),
        "griffin",
        10,
        &[],
    )
    .await
    .expect("character-scoped keyword search should not error");
    assert_eq!(
        hits_char.len(),
        2,
        "character scope should span both conversations"
    );

    // exclude_message_ids should remove the excluded id from results.
    let hits_excluded = EmbeddingRepo::keyword_search_messages(
        &db,
        None,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &character.id,
        )),
        "griffin",
        10,
        &[janus_lib::db::value_bridge::record_id_to_string(
            &griffin_b.id,
        )],
    )
    .await
    .expect("keyword search with exclusion should not error");
    assert_eq!(
        hits_excluded.len(),
        1,
        "excluded message should be filtered out"
    );
    assert!(!hits_excluded
        .iter()
        .any(|h| h.message_id == janus_lib::db::value_bridge::record_id_to_string(&griffin_b.id)));

    cleanup(dir);
}

/// Regression test for the sidebar search feature (`ConversationRepo::
/// search_messages`), which had the exact same "ORDER BY must reference an
/// alias" bug — pre-existing, found while debugging the hybrid search
/// queries above, fixed alongside them.
#[tokio::test]
async fn search_messages_finds_exact_term() {
    let (db, dir) = test_db().await;

    let character = CharacterRepo::create(&db, "Elara", serde_json::json!({"name": "Elara"}))
        .await
        .expect("create character");
    let conv = ConversationRepo::create(
        &db,
        Some(&janus_lib::db::value_bridge::record_id_to_string(
            &character.id,
        )),
        Some("Conv"),
        None,
    )
    .await
    .expect("create conversation");

    MessageRepo::create(
        &db,
        &janus_lib::db::value_bridge::record_id_to_string(&conv.id),
        "user",
        "I saw a griffin over the mountains",
        None,
        None,
    )
    .await
    .expect("create message");
    MessageRepo::create(
        &db,
        &janus_lib::db::value_bridge::record_id_to_string(&conv.id),
        "assistant",
        "That sounds like an ordinary hawk to me",
        None,
        None,
    )
    .await
    .expect("create unrelated message");

    let results = ConversationRepo::search_messages(&db, "griffin", 10)
        .await
        .expect("search_messages should not error");
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("griffin"));

    cleanup(dir);
}

#[tokio::test]
async fn keyword_search_memories_finds_exact_term() {
    let (db, dir) = test_db().await;

    let character = CharacterRepo::create(&db, "Kael", serde_json::json!({"name": "Kael"}))
        .await
        .expect("create character");
    let char_id = janus_lib::db::value_bridge::record_id_to_string(&character.id);

    MemoryRepo::create(
        &db,
        Some(&char_id),
        None,
        "Kael is afraid of thunderstorms",
        "user",
    )
    .await
    .expect("create memory 1");
    MemoryRepo::create(
        &db,
        Some(&char_id),
        None,
        "Kael prefers tea over coffee",
        "user",
    )
    .await
    .expect("create memory 2");

    let hits = EmbeddingRepo::keyword_search_memories(&db, &char_id, "thunderstorms", 10, None)
        .await
        .expect("keyword memory search should not error");
    assert_eq!(hits.len(), 1);
    assert!(hits[0].content.contains("thunderstorms"));

    let no_hits = EmbeddingRepo::keyword_search_memories(&db, &char_id, "spaceship", 10, None)
        .await
        .expect("keyword memory search for absent term should not error");
    assert_eq!(no_hits.len(), 0);

    cleanup(dir);
}
