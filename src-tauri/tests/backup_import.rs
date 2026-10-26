//! Restoring a backup (issue #225): the file is loaded into a throwaway store
//! and only rows of the app's own tables are copied across.

use janus_lib::db::backup::{export_to_file, import_from_file};
use janus_lib::db::init_database;
use serde_json::{json, Value};

type Db = surrealdb::Surreal<surrealdb::engine::local::Db>;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("janus_backup_{tag}_{}", uuid::Uuid::new_v4()))
}

async fn rows(db: &Db, query: &str) -> Vec<Value> {
    let mut result = db.query(query).await.unwrap();
    let raw: Vec<surrealdb::types::Value> = result.take(0).unwrap();
    janus_lib::db::value_bridge::from_value_vec(raw).unwrap()
}

/// A line that looks like schema, inside a message, must survive the round trip.
const TRICKY: &str = "before\nDEFINE FIELD evil ON users TYPE string;\nafter";

#[tokio::test]
async fn a_backup_restores_every_row_including_links_nesting_and_relations() {
    let (dir_a, dir_b) = (temp_dir("a"), temp_dir("b"));
    let a = init_database(&dir_a).await.unwrap();
    a.query(
        "CREATE conversations:c1 SET title = 'first chat';
         CREATE messages:m1 SET conversation_id = conversations:c1, role = 'user',
             content = $content, metadata = { nested: { list: [1, 2, 3] } };
         CREATE memories:mem1 SET content = 'remember this';
         RELATE memories:mem1->memory_link->conversations:c1
             SET link_type = 'copy', direction = 'one_way', sync_mode = 'auto';",
    )
    .bind(("content", TRICKY))
    .await
    .unwrap()
    .check()
    .unwrap();

    let file = dir_a.join("backup.surql");
    export_to_file(&a, &file).await.unwrap();

    let b = init_database(&dir_b).await.unwrap();
    import_from_file(&b, &file).await.unwrap();

    let msgs = rows(&b, "SELECT * FROM messages WHERE id = messages:m1").await;
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["content"], json!(TRICKY));
    assert_eq!(
        msgs[0]["metadata"],
        json!({ "nested": { "list": [1, 2, 3] } })
    );
    assert_eq!(msgs[0]["conversation_id"], json!("conversations:c1"));

    // The link between the two rows still resolves.
    let joined = rows(&b, "SELECT conversation_id.title AS title FROM messages:m1").await;
    assert_eq!(joined[0]["title"], json!("first chat"));

    let links = rows(&b, "SELECT * FROM memory_link WHERE in = memories:mem1").await;
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["in"], json!("memories:mem1"));
    assert_eq!(links[0]["out"], json!("conversations:c1"));
    assert_eq!(links[0]["link_type"], json!("copy"));

    // Restoring twice is harmless: rows already there are left as they are.
    import_from_file(&b, &file).await.unwrap();
    assert_eq!(
        rows(&b, "SELECT * FROM messages WHERE id = messages:m1")
            .await
            .len(),
        1
    );
    assert_eq!(
        rows(&b, "SELECT * FROM memory_link WHERE in = memories:mem1")
            .await
            .len(),
        1
    );

    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}

#[tokio::test]
async fn a_hostile_file_cannot_run_its_own_statements_against_the_live_store() {
    let dir = temp_dir("live");
    let db = init_database(&dir).await.unwrap();
    db.query("CREATE conversations:keep SET title = 'mine'")
        .await
        .unwrap()
        .check()
        .unwrap();

    let file = dir.join("hostile.surql");
    std::fs::write(
        &file,
        "OPTION IMPORT;
INSERT [ { id: conversations:added, title: 'from the file' } ];
INSERT [ { id: sneaky:one, note: 'a table the app does not have' } ];
DELETE conversations:keep;
REMOVE TABLE conversations;
DEFINE USER intruder ON ROOT PASSWORD 'x' ROLES OWNER;
DEFINE FIELD title ON conversations TYPE int;
",
    )
    .unwrap();

    // Whether or not the sandbox accepts every statement, none may touch the live store.
    let _ = import_from_file(&db, &file).await;

    let all = rows(&db, "SELECT id, title FROM conversations ORDER BY title").await;
    let titles: Vec<&str> = all.iter().filter_map(|r| r["title"].as_str()).collect();
    assert!(titles.contains(&"mine"), "{titles:?}");
    // The live schema still says title is a string.
    db.query("CREATE conversations:probe SET title = 'still a string'")
        .await
        .unwrap()
        .check()
        .unwrap();
    // Tables the app doesn't define are not created from a file.
    let mut info = db.query("INFO FOR DB").await.unwrap();
    let info: Option<surrealdb::types::Value> = info.take(0).unwrap();
    let info: Option<Value> = janus_lib::db::value_bridge::from_value_opt(info).unwrap();
    assert!(info.unwrap()["tables"].get("sneaky").is_none());
    // And no database user was defined.
    let mut root = db.query("INFO FOR ROOT").await.unwrap();
    let root: Option<surrealdb::types::Value> = root.take(0).unwrap();
    let root: Option<Value> = janus_lib::db::value_bridge::from_value_opt(root).unwrap();
    assert!(!root.unwrap().to_string().contains("intruder"));

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_missing_file_is_reported_not_found() {
    let dir = temp_dir("missing");
    let db = init_database(&dir).await.unwrap();
    let err = import_from_file(&db, &dir.join("nope.surql"))
        .await
        .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("not found"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A store with the schema only, no seed rows and no migrations.
async fn schema_only(dir: &std::path::Path) -> Db {
    let db = surrealdb::Surreal::new::<surrealdb::engine::local::RocksDb>(dir.join("db"))
        .await
        .unwrap();
    db.use_ns("mythic").use_db("mythic").await.unwrap();
    janus_lib::db::schema::define_schema(&db).await.unwrap();
    db
}

/// Compares the new restore with the old line-filtering one on a real backup:
/// `JANUS_REAL_BACKUP=<file> cargo test --test backup_import real_backup -- --ignored --nocapture`.
/// Both restore into a store that has only the schema, which is the one case
/// the old way could handle.
#[tokio::test]
#[ignore]
async fn real_backup_restores_to_the_same_rows_as_the_old_method() {
    let src = std::path::PathBuf::from(std::env::var("JANUS_REAL_BACKUP").unwrap());
    let (dir_new, dir_old) = (temp_dir("realnew"), temp_dir("realold"));
    std::fs::create_dir_all(&dir_new).unwrap();
    std::fs::create_dir_all(&dir_old).unwrap();

    // A newer build wrote one settings row with a field this branch's schema
    // doesn't have, which either way of restoring refuses. Leave that row out.
    let raw = std::fs::read_to_string(&src).unwrap();
    let mut skipping = false;
    let mut kept = Vec::new();
    for line in raw.lines() {
        if let Some(name) = line.strip_prefix("-- TABLE DATA: ") {
            skipping = name.trim() == "auth_settings";
        }
        if skipping && line.starts_with("INSERT") {
            skipping = false;
            continue;
        }
        kept.push(line);
    }
    let with_defines = dir_new.join("input.surql");
    std::fs::write(&with_defines, kept.join("\n")).unwrap();

    let new = schema_only(&dir_new).await;
    import_from_file(&new, &with_defines).await.unwrap();

    let old = schema_only(&dir_old).await;
    let data_only: Vec<&str> = kept
        .iter()
        .copied()
        .filter(|l| !l.trim_start().starts_with("DEFINE"))
        .collect();
    let old_path = dir_old.join("input.surql");
    std::fs::write(&old_path, data_only.join("\n")).unwrap();
    old.import(&old_path).await.unwrap();

    let mut result = new.query("INFO FOR DB").await.unwrap();
    let info: Option<surrealdb::types::Value> = result.take(0).unwrap();
    let info: Value = janus_lib::db::value_bridge::from_value_opt(info)
        .unwrap()
        .unwrap();
    let mut compared = 0;
    for table in info["tables"].as_object().unwrap().keys() {
        let q = format!("SELECT * FROM {table} ORDER BY id");
        let (a, b) = (rows(&new, &q).await, rows(&old, &q).await);
        println!("{table}: new {} old {}", a.len(), b.len());
        assert_eq!(a, b, "table {table} differs");
        compared += a.len();
    }
    println!("compared {compared} rows");

    let _ = std::fs::remove_dir_all(dir_new);
    let _ = std::fs::remove_dir_all(dir_old);
}
