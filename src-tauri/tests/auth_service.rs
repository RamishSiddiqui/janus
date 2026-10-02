//! Integration tests for accounts, sessions, lockout and recovery (#94),
//! against a real embedded SurrealDB so the actual queries and schema
//! constraints are exercised.

use janus_lib::auth::service as auth;
use janus_lib::db::init_database;
use janus_lib::error::MythicError;
use janus_lib::models::user::{Role, SignupMode, UserInfo};

type TestDb = surrealdb::Surreal<surrealdb::engine::local::Db>;

async fn test_db() -> (TestDb, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mythic_test_{}", uuid::Uuid::new_v4()));
    let db = init_database(&dir).await.expect("init_database");
    (db, dir)
}

fn cleanup(dir: std::path::PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

const PASS: &str = "correct horse battery";

fn is_unauthorized(e: &MythicError) -> bool {
    matches!(e, MythicError::Unauthorized(_))
}

#[tokio::test]
async fn first_account_is_admin_then_signup_follows_the_setting() {
    let (db, dir) = test_db().await;

    assert!(!auth::has_users(&db).await.unwrap());
    let (admin, key) = auth::register(&db, "Ramish", PASS).await.unwrap();
    assert_eq!(admin.role, Role::Admin);
    assert_eq!(admin.username, "ramish");
    assert!(key.starts_with("JANUS-"));
    let admin_info = UserInfo::from(&admin);

    // Default is admin-only: a second visitor is turned away.
    assert_eq!(auth::signup_mode(&db).await.unwrap(), SignupMode::AdminOnly);
    let err = auth::register(&db, "guest", PASS).await.unwrap_err();
    assert!(is_unauthorized(&err));

    // Open signups let them in as a member.
    auth::set_signup_mode(&db, &admin_info, SignupMode::Open)
        .await
        .unwrap();
    let (member, _) = auth::register(&db, "guest", PASS).await.unwrap();
    assert_eq!(member.role, Role::Member);

    // Usernames are unique, and a member can't flip the setting.
    assert!(auth::register(&db, "GUEST", PASS).await.is_err());
    let member_info = UserInfo::from(&member);
    assert!(
        auth::set_signup_mode(&db, &member_info, SignupMode::AdminOnly)
            .await
            .is_err()
    );

    cleanup(dir);
}

#[tokio::test]
async fn login_sessions_and_logout() {
    let (db, dir) = test_db().await;
    auth::register(&db, "ramish", PASS).await.unwrap();

    assert!(auth::login(&db, "ramish", "wrong passphrase!", false, "t")
        .await
        .is_err());
    let (user, token) = auth::login(&db, "Ramish", PASS, true, "Chrome on Windows")
        .await
        .unwrap();

    let (found, session) = auth::authenticate(&db, &token).await.unwrap().unwrap();
    assert_eq!(found.id, user.id);
    assert!(session.trusted);
    assert!(auth::authenticate(&db, "not-a-token")
        .await
        .unwrap()
        .is_none());

    let sessions = auth::list_sessions(&db, &user.id, Some(&session.id))
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].current);

    auth::logout(&db, &token).await.unwrap();
    assert!(auth::authenticate(&db, &token).await.unwrap().is_none());

    cleanup(dir);
}

#[tokio::test]
async fn three_wrong_passphrases_lock_the_account() {
    let (db, dir) = test_db().await;
    auth::register(&db, "ramish", PASS).await.unwrap();

    for _ in 0..3 {
        let err = auth::verify_credentials(&db, "ramish", "nope nope nope nope")
            .await
            .unwrap_err();
        assert!(is_unauthorized(&err));
    }
    // Even the right passphrase is refused while locked.
    let err = auth::verify_credentials(&db, "ramish", PASS)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Try again in"), "{err}");

    // Unknown usernames get the same generic answer.
    let err = auth::verify_credentials(&db, "nobody", PASS)
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "That username and passphrase don't match.");

    cleanup(dir);
}

#[tokio::test]
async fn recovery_key_resets_passphrase_and_is_single_use() {
    let (db, dir) = test_db().await;
    let (_, key) = auth::register(&db, "ramish", PASS).await.unwrap();
    let (_, token) = auth::login(&db, "ramish", PASS, false, "t").await.unwrap();

    let sloppy = key.to_lowercase().replace('-', " ");
    let (user, new_key) =
        auth::reset_with_recovery(&db, "ramish", &sloppy, "a brand new passphrase")
            .await
            .unwrap();
    assert_ne!(new_key, key);

    // Old passphrase and old sessions are gone; the new passphrase works.
    assert!(auth::authenticate(&db, &token).await.unwrap().is_none());
    assert!(auth::verify_credentials(&db, "ramish", PASS).await.is_err());
    let ok = auth::verify_credentials(&db, "ramish", "a brand new passphrase")
        .await
        .unwrap();
    assert_eq!(ok.id, user.id);

    // The spent key no longer works.
    assert!(
        auth::reset_with_recovery(&db, "ramish", &key, "another passphrase here")
            .await
            .is_err()
    );
    // The new one does.
    assert!(
        auth::reset_with_recovery(&db, "ramish", &new_key, "yet another passphrase")
            .await
            .is_ok()
    );

    cleanup(dir);
}

#[tokio::test]
async fn admin_adds_members_and_account_guards_hold() {
    let (db, dir) = test_db().await;
    let (admin, _) = auth::register(&db, "ramish", PASS).await.unwrap();
    let admin_info = UserInfo::from(&admin);

    let (member, _) = auth::admin_create_user(&db, &admin_info, "sara", "temporary passphrase")
        .await
        .unwrap();
    assert!(UserInfo::from(&member).must_change);
    assert_eq!(auth::list_users(&db, &admin_info).await.unwrap().len(), 2);

    // Choosing their own passphrase clears the flag.
    auth::change_passphrase(
        &db,
        &member.id,
        "temporary passphrase",
        "sara picked this one",
        None,
    )
    .await
    .unwrap();
    let again = auth::verify_credentials(&db, "sara", "sara picked this one")
        .await
        .unwrap();
    assert!(!again.must_change);

    // Members can't manage accounts; the last admin and yourself can't be deleted.
    let member_info = UserInfo::from(&member);
    assert!(
        auth::admin_create_user(&db, &member_info, "eve", "some passphrase here")
            .await
            .is_err()
    );
    assert!(auth::list_users(&db, &member_info).await.is_err());
    assert!(auth::delete_user(&db, &admin_info, &admin.id)
        .await
        .is_err());

    auth::delete_user(&db, &admin_info, &member.id)
        .await
        .unwrap();
    assert_eq!(auth::list_users(&db, &admin_info).await.unwrap().len(), 1);

    cleanup(dir);
}
