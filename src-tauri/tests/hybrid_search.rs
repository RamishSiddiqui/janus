//! Integration test for the hybrid (BM25 + vector) retrieval query layer
//! added for RRF-based RAG. Runs against a real embedded SurrealDB instance
//! (same engine the app uses) to exercise the actual SurrealQL — the `@1@`
//! match operator and `search::score()` — rather than just compiling it.
//!
//! This intentionally only covers `EmbeddingRepo::keyword_search_messages`
//! and `keyword_search_memories` (the new BM25 query paths), since the
//! vector-similarity half requires a live embedding provider and is
//! exercised indirectly through those code paths already.

use mythic_lib::db::characters::CharacterRepo;
use mythic_lib::db::conversations::ConversationRepo;
use mythic_lib::db::embeddings::EmbeddingRepo;
use mythic_lib::db::memories::MemoryRepo;
use mythic_lib::db::messages::MessageRepo;
use mythic_lib::db::init_database;

async fn test_db() -> (surrealdb::Surreal<surrealdb::engine::local::Db>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir).await.expect("init_database should succeed");
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

    let conv_a = ConversationRepo::create(&db, Some(&character.id.id.to_raw()), Some("Conv A"))
        .await
        .expect("create conversation A");
    let conv_b = ConversationRepo::create(&db, Some(&character.id.id.to_raw()), Some("Conv B"))
        .await
        .expect("create conversation B");

    MessageRepo::create(&db, &conv_a.id.id.to_raw(), "user", "I saw a griffin over the mountains", None, None)
        .await
        .expect("create message with distinctive term");
    MessageRepo::create(&db, &conv_a.id.id.to_raw(), "assistant", "That sounds like an ordinary hawk to me", None, None)
        .await
        .expect("create unrelated message");
    let griffin_b = MessageRepo::create(&db, &conv_b.id.id.to_raw(), "user", "Another griffin sighting near the tower", None, None)
        .await
        .expect("create message in conversation B");

    // Conversation-scoped: only conv_a's griffin message should come back.
    let hits_a = EmbeddingRepo::keyword_search_messages(&db, Some(&conv_a.id.id.to_raw()), None, "griffin", 10, &[])
        .await
        .expect("keyword search in conv_a should not error");
    assert_eq!(hits_a.len(), 1, "expected exactly one griffin match scoped to conv_a");
    assert!(hits_a[0].content.contains("griffin"));

    // The unrelated message in the same conversation must not match.
    let hits_hawk = EmbeddingRepo::keyword_search_messages(&db, Some(&conv_a.id.id.to_raw()), None, "hawk", 10, &[])
        .await
        .expect("keyword search for hawk should not error");
    assert_eq!(hits_hawk.len(), 1);
    assert!(hits_hawk[0].content.contains("hawk"));

    // Character-scoped (no conversation filter): both conversations' griffin
    // messages should be found since they share the same character.
    let hits_char = EmbeddingRepo::keyword_search_messages(&db, None, Some(&character.id.id.to_raw()), "griffin", 10, &[])
        .await
        .expect("character-scoped keyword search should not error");
    assert_eq!(hits_char.len(), 2, "character scope should span both conversations");

    // exclude_message_ids should remove the excluded id from results.
    let hits_excluded = EmbeddingRepo::keyword_search_messages(
        &db, None, Some(&character.id.id.to_raw()), "griffin", 10,
        &[griffin_b.id.id.to_raw()],
    )
    .await
    .expect("keyword search with exclusion should not error");
    assert_eq!(hits_excluded.len(), 1, "excluded message should be filtered out");
    assert!(!hits_excluded.iter().any(|h| h.message_id == griffin_b.id.id.to_raw()));

    cleanup(dir);
}

#[tokio::test]
async fn keyword_search_memories_finds_exact_term() {
    let (db, dir) = test_db().await;

    let character = CharacterRepo::create(&db, "Kael", serde_json::json!({"name": "Kael"}))
        .await
        .expect("create character");
    let char_id = character.id.id.to_raw();

    MemoryRepo::create(&db, Some(&char_id), None, "Kael is afraid of thunderstorms", "user")
        .await
        .expect("create memory 1");
    MemoryRepo::create(&db, Some(&char_id), None, "Kael prefers tea over coffee", "user")
        .await
        .expect("create memory 2");

    let hits = EmbeddingRepo::keyword_search_memories(&db, &char_id, "thunderstorms", 10)
        .await
        .expect("keyword memory search should not error");
    assert_eq!(hits.len(), 1);
    assert!(hits[0].content.contains("thunderstorms"));

    let no_hits = EmbeddingRepo::keyword_search_memories(&db, &char_id, "spaceship", 10)
        .await
        .expect("keyword memory search for absent term should not error");
    assert_eq!(no_hits.len(), 0);

    cleanup(dir);
}
