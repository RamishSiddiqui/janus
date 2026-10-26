//! Provider API keys are encrypted at rest and masked towards screens (issue #227).

use janus_lib::db::backup::export_to_file;
use janus_lib::db::init_database;
use janus_lib::db::providers::ProviderRepo;
use serde_json::{json, Value};

type Db = surrealdb::Surreal<surrealdb::engine::local::Db>;

const KEY: &str = "sk-or-v1-0123456789abcdefWXYZ";

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("janus_keys_{tag}_{}", uuid::Uuid::new_v4()))
}

/// The `config` object exactly as it sits in the database.
async fn stored_config(db: &Db, id: &str) -> Value {
    let mut result = db
        .query("SELECT config FROM type::record('provider_configs', $id)")
        .bind(("id", id.to_string()))
        .await
        .unwrap();
    let raw: Vec<surrealdb::types::Value> = result.take(0).unwrap();
    let rows: Vec<Value> = janus_lib::db::value_bridge::from_value_vec(raw).unwrap();
    rows[0]["config"].clone()
}

fn key_of(id: &surrealdb::types::RecordId) -> String {
    janus_lib::db::value_bridge::record_id_to_string(id)
}

async fn make(db: &Db, key: &str) -> janus_lib::models::provider::ProviderConfig {
    ProviderRepo::create(
        db,
        "Test provider",
        "llm",
        "open_router",
        json!({ "base_url": "https://example.test/v1", "api_key": key }),
        false,
        "",
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_saved_key_is_encrypted_in_the_database_and_plain_everywhere_the_app_reads_it() {
    let dir = temp_dir("saved");
    let db = init_database(&dir).await.unwrap();
    let created = make(&db, KEY).await;
    let id = key_of(&created.id);

    // What the rest of the app gets is the real key.
    assert_eq!(created.config["api_key"], json!(KEY));
    assert_eq!(
        ProviderRepo::get(&db, &id).await.unwrap().config["api_key"],
        json!(KEY)
    );
    let listed = ProviderRepo::list(&db, None, None).await.unwrap();
    assert!(listed.iter().any(|p| p.config["api_key"] == json!(KEY)));

    // What is on disk is not.
    let at_rest = stored_config(&db, &id).await;
    let stored = at_rest["api_key"].as_str().unwrap();
    assert!(stored.starts_with("enc:v1:"), "{stored}");
    assert!(!stored.contains("0123456789"));
    assert_eq!(at_rest["base_url"], json!("https://example.test/v1"));

    // What a screen or a browser gets is masked.
    let masked = created.clone().masked();
    let shown = masked.config["api_key"].as_str().unwrap();
    assert!(
        shown.ends_with("WXYZ") && !shown.contains("0123"),
        "{shown}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn saving_the_masked_key_back_keeps_the_stored_key_and_an_empty_one_clears_it() {
    let dir = temp_dir("update");
    let db = init_database(&dir).await.unwrap();
    let created = make(&db, KEY).await;
    let id = key_of(&created.id);
    let masked = created.clone().masked().config["api_key"]
        .as_str()
        .unwrap()
        .to_string();

    // Another field changed; the screen sends the key back as it was shown.
    let updated = ProviderRepo::update(
        &db,
        &id,
        None,
        Some(json!({ "base_url": "https://changed.test/v1", "api_key": masked })),
    )
    .await
    .unwrap();
    assert_eq!(updated.config["api_key"], json!(KEY));
    assert_eq!(updated.config["base_url"], json!("https://changed.test/v1"));
    assert!(stored_config(&db, &id).await["api_key"]
        .as_str()
        .unwrap()
        .starts_with("enc:v1:"));

    // A newly typed key replaces it.
    let replaced = ProviderRepo::update(
        &db,
        &id,
        None,
        Some(json!({ "api_key": "sk-new-key-1234567890" })),
    )
    .await
    .unwrap();
    assert_eq!(replaced.config["api_key"], json!("sk-new-key-1234567890"));

    // Clearing the field clears the key.
    let cleared = ProviderRepo::update(&db, &id, None, Some(json!({ "api_key": "" })))
        .await
        .unwrap();
    assert_eq!(cleared.config["api_key"], json!(""));
    assert_eq!(stored_config(&db, &id).await["api_key"], json!(""));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn keys_saved_by_an_older_version_are_encrypted_once_at_startup() {
    let dir = temp_dir("legacy");
    let db = init_database(&dir).await.unwrap();
    db.query(
        "CREATE provider_configs:legacy1 CONTENT {
            name: 'Old', provider_type: 'llm', adapter: 'open_router',
            config: { api_key: $key, model: 'm' }, is_default: false, owner_id: ''
         }",
    )
    .bind(("key", KEY))
    .await
    .unwrap()
    .check()
    .unwrap();
    assert_eq!(stored_config(&db, "legacy1").await["api_key"], json!(KEY));
    // Readable as it is before the migration runs.
    assert_eq!(
        ProviderRepo::get(&db, "legacy1").await.unwrap().config["api_key"],
        json!(KEY)
    );

    assert_eq!(ProviderRepo::protect_existing(&db).await.unwrap(), 1);
    let at_rest = stored_config(&db, "legacy1").await;
    assert!(at_rest["api_key"].as_str().unwrap().starts_with("enc:v1:"));
    assert_eq!(at_rest["model"], json!("m"));
    assert_eq!(
        ProviderRepo::get(&db, "legacy1").await.unwrap().config["api_key"],
        json!(KEY)
    );
    // Nothing left to do the second time.
    assert_eq!(ProviderRepo::protect_existing(&db).await.unwrap(), 0);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_key_that_cannot_be_opened_here_is_flagged_instead_of_failing_the_list() {
    let dir = temp_dir("foreign");
    let db = init_database(&dir).await.unwrap();
    let mine = make(&db, KEY).await;
    let other = make(&db, "sk-other-key-abcdefghij").await;
    let (mine_id, other_id) = (key_of(&mine.id), key_of(&other.id));

    // A ciphertext moved onto another row, and one from another computer.
    let moved = stored_config(&db, &mine_id).await["api_key"].clone();
    db.query("UPDATE type::record('provider_configs', $id) SET config.api_key = $k")
        .bind(("id", other_id.clone()))
        .bind(("k", moved.as_str().unwrap().to_string()))
        .await
        .unwrap()
        .check()
        .unwrap();

    let got = ProviderRepo::get(&db, &other_id).await.unwrap();
    assert_eq!(got.config["api_key"], json!(""));
    assert_eq!(got.config["api_key_unreadable"], json!(true));
    // The row it was copied from is unaffected.
    assert_eq!(
        ProviderRepo::get(&db, &mine_id).await.unwrap().config["api_key"],
        json!(KEY)
    );
    assert!(ProviderRepo::list(&db, None, None).await.unwrap().len() >= 2);

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_backup_carries_the_encrypted_key_not_the_real_one() {
    let dir = temp_dir("backup");
    let db = init_database(&dir).await.unwrap();
    make(&db, KEY).await;
    let file = dir.join("backup.surql");
    export_to_file(&db, &file).await.unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("enc:v1:"));
    assert!(
        !text.contains(KEY),
        "the backup holds the key in plain text"
    );
    assert!(!text.contains("0123456789abcdef"));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_backup_restored_on_the_same_computer_brings_the_keys_back_working() {
    let (dir_a, dir_b) = (temp_dir("from"), temp_dir("to"));
    let a = init_database(&dir_a).await.unwrap();
    let created = make(&a, KEY).await;
    let id = key_of(&created.id);
    let file = dir_a.join("backup.surql");
    export_to_file(&a, &file).await.unwrap();

    let b = init_database(&dir_b).await.unwrap();
    janus_lib::db::backup::import_from_file(&b, &file)
        .await
        .unwrap();
    let restored = ProviderRepo::get(&b, &id).await.unwrap();
    assert_eq!(restored.config["api_key"], json!(KEY));
    assert!(restored.config.get("api_key_unreadable").is_none());

    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}
