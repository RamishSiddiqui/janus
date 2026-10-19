//! A legacy row (written before a defaulted field existed) must not block the
//! account backfill: SurrealDB validates the whole row on every write.

use janus_lib::db::init_database;
use janus_lib::db::migrations::backfill_defaults;

#[tokio::test]
async fn backfill_fills_every_defaulted_field_of_a_legacy_row() {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir).await.unwrap();

    db.query(
        "
        DEFINE TABLE legacy_t SCHEMAFULL;
        DEFINE FIELD name ON legacy_t TYPE string;
        CREATE legacy_t:old SET name = 'old';
        DEFINE FIELD count ON legacy_t TYPE int DEFAULT 0;
        DEFINE FIELD owner_id ON legacy_t TYPE string DEFAULT '';
        DEFINE FIELD note ON legacy_t TYPE option<string>;
        ",
    )
    .await
    .unwrap()
    .check()
    .unwrap();

    backfill_defaults(&db, "legacy_t").await.unwrap();

    let mut r = db
        .query("SELECT count, owner_id FROM legacy_t:old")
        .await
        .unwrap();
    let rows: Vec<surrealdb::types::Value> = r.take(0).unwrap();
    let rows: Vec<serde_json::Value> = janus_lib::db::value_bridge::from_value_vec(rows).unwrap();
    assert_eq!(rows[0]["count"], 0);
    assert_eq!(rows[0]["owner_id"], "");

    // The row now accepts ordinary updates again.
    db.query("UPDATE legacy_t:old SET name = 'renamed'")
        .await
        .unwrap()
        .check()
        .unwrap();

    let _ = std::fs::remove_dir_all(dir);
}
