//! Two accounts must not be able to see or touch each other's data (#94,
//! slice 2b). Command bodies can't be unit-tested without a Tauri runtime, so
//! this exercises exactly what they call: the `ensure_*` checks, the owner
//! stamping, and the owner-filtered list queries.

use janus_lib::auth::access::{
    ensure_character, ensure_conversation, ensure_message, ensure_persona, ensure_scene,
    inherit_owner_from_conversation, resolve_actor, stamp_owner, Actor,
};
use janus_lib::auth::service as auth;
use janus_lib::db::characters::CharacterRepo;
use janus_lib::db::conversations::ConversationRepo;
use janus_lib::db::init_database;
use janus_lib::db::messages::MessageRepo;
use janus_lib::db::personas::PersonaRepo;
use janus_lib::db::scenes::SceneRepo;
use janus_lib::db::value_bridge::record_id_to_string;
use janus_lib::error::MythicError;
use janus_lib::models::user::{SignupMode, UserInfo};

const PASS: &str = "correct horse battery";

fn not_found(r: Result<(), MythicError>) -> bool {
    matches!(r, Err(MythicError::NotFound(_)))
}

#[tokio::test]
async fn one_account_cannot_reach_another_accounts_data() {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir).await.unwrap();

    let (ada_row, _) = auth::register(&db, "ada", PASS).await.unwrap();
    auth::set_signup_mode(&db, &UserInfo::from(&ada_row), SignupMode::Open)
        .await
        .unwrap();
    let (bob_row, _) = auth::register(&db, "bob", PASS).await.unwrap();
    let ada = resolve_actor(&db, Some(&ada_row.id)).await.unwrap();
    let bob = resolve_actor(&db, Some(&bob_row.id)).await.unwrap();

    // Debug builds seed demo data, which the first account (Ada) claims, so
    // compare against what she already has instead of assuming an empty library.
    let base_chars = CharacterRepo::list(&db, ada.owner_filter())
        .await
        .unwrap()
        .len();
    let base_personas = PersonaRepo::list(&db, ada.owner_filter())
        .await
        .unwrap()
        .len();
    let base_convs = ConversationRepo::list(&db, 500, 0, ada.owner_filter())
        .await
        .unwrap()
        .len();
    let base_count = ConversationRepo::count(&db, ada.owner_filter())
        .await
        .unwrap();
    let base_trash = ConversationRepo::list_trashed(&db, ada.owner_filter())
        .await
        .unwrap()
        .len();

    // Ada creates one of everything, the way the commands do: create, then stamp.
    let ch = CharacterRepo::create(&db, "Elara", serde_json::json!({"name": "Elara"}))
        .await
        .unwrap();
    let ch_id = record_id_to_string(&ch.id);
    stamp_owner(&db, &ada, "characters", &ch_id).await.unwrap();

    let pe = PersonaRepo::create(&db, "Ada", serde_json::json!({"name": "Ada"}))
        .await
        .unwrap();
    let pe_id = record_id_to_string(&pe.id);
    stamp_owner(&db, &ada, "personas", &pe_id).await.unwrap();

    let conv = ConversationRepo::create(&db, Some(&ch_id), Some("Secret chat"), Some(&pe_id))
        .await
        .unwrap();
    let conv_id = record_id_to_string(&conv.id);
    stamp_owner(&db, &ada, "conversations", &conv_id)
        .await
        .unwrap();

    let msg = MessageRepo::create(
        &db,
        &conv_id,
        "user",
        "the griffin flies at dawn",
        None,
        None,
    )
    .await
    .unwrap();
    let msg_id = record_id_to_string(&msg.id);

    let scene_id = uuid::Uuid::new_v4().to_string();
    SceneRepo::create(
        &db,
        &scene_id,
        &conv_id,
        None,
        "image",
        "p",
        "scenes/x.png",
        None,
        None,
    )
    .await
    .unwrap();

    // Ada passes every check.
    ensure_character(&db, &ada, &ch_id).await.unwrap();
    ensure_persona(&db, &ada, &pe_id).await.unwrap();
    ensure_conversation(&db, &ada, &conv_id).await.unwrap();
    ensure_message(&db, &ada, &msg_id).await.unwrap();
    ensure_scene(&db, &ada, &scene_id).await.unwrap();

    // Bob is refused on every one, with "not found".
    assert!(not_found(ensure_character(&db, &bob, &ch_id).await));
    assert!(not_found(ensure_persona(&db, &bob, &pe_id).await));
    assert!(not_found(ensure_conversation(&db, &bob, &conv_id).await));
    assert!(not_found(ensure_message(&db, &bob, &msg_id).await));
    assert!(not_found(ensure_scene(&db, &bob, &scene_id).await));

    // Lists: Ada sees hers, Bob sees none.
    assert_eq!(
        CharacterRepo::list(&db, ada.owner_filter())
            .await
            .unwrap()
            .len(),
        base_chars + 1
    );
    assert!(CharacterRepo::list(&db, bob.owner_filter())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        PersonaRepo::list(&db, ada.owner_filter())
            .await
            .unwrap()
            .len(),
        base_personas + 1
    );
    assert!(PersonaRepo::list(&db, bob.owner_filter())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        ConversationRepo::list(&db, 500, 0, ada.owner_filter())
            .await
            .unwrap()
            .len(),
        base_convs + 1
    );
    assert!(ConversationRepo::list(&db, 500, 0, bob.owner_filter())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        ConversationRepo::count(&db, ada.owner_filter())
            .await
            .unwrap(),
        base_count + 1
    );
    assert_eq!(
        ConversationRepo::count(&db, bob.owner_filter())
            .await
            .unwrap(),
        0
    );

    // Search: Ada finds her message, Bob finds nothing for the same query.
    let ada_hits = ConversationRepo::search_messages(&db, "griffin", 10, ada.owner_filter())
        .await
        .unwrap();
    assert!(!ada_hits.is_empty());
    let bob_hits = ConversationRepo::search_messages(&db, "griffin", 10, bob.owner_filter())
        .await
        .unwrap();
    assert!(bob_hits.is_empty());

    // Trash is per account too.
    ConversationRepo::trash(&db, &conv_id).await.unwrap();
    assert_eq!(
        ConversationRepo::list_trashed(&db, ada.owner_filter())
            .await
            .unwrap()
            .len(),
        base_trash + 1
    );
    assert!(ConversationRepo::list_trashed(&db, bob.owner_filter())
        .await
        .unwrap()
        .is_empty());

    // An NPC the pipeline creates inherits the conversation's owner.
    let npc = CharacterRepo::create_npc(&db, "Innkeeper", serde_json::json!({"name": "Innkeeper"}))
        .await
        .unwrap();
    let npc_id = record_id_to_string(&npc.id);
    inherit_owner_from_conversation(&db, &conv_id, "characters", &npc_id)
        .await
        .unwrap();
    ensure_character(&db, &ada, &npc_id).await.unwrap();
    assert!(not_found(ensure_character(&db, &bob, &npc_id).await));

    // Legacy mode (no accounts) filters nothing; signed-out is refused once accounts exist.
    assert!(Actor::Legacy.owner_filter().is_none());
    assert!(resolve_actor(&db, None).await.is_err());

    let _ = std::fs::remove_dir_all(dir);
}
