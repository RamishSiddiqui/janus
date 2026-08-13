//! Verifies the tauri-specta command registry actually exports valid
//! TypeScript without needing to launch the full Tauri app (which needs a
//! window/webview and isn't practical to drive in CI or this environment).
//!
//! If this test fails to compile, it usually means a command or type was
//! added to the specta registry without a matching `specta::Type` derive or
//! `#[specta(type = ...)]` override for a field that doesn't serialize as
//! its literal Rust type (e.g. SurrealDB's `Thing`, which is sent over IPC
//! as a plain string).

#[test]
fn specta_registry_exports_valid_typescript() {
    let dir = std::env::temp_dir().join(format!("mythic_bindings_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let out_path = dir.join("bindings.ts");

    janus_lib::specta_builder()
        .export(specta_typescript::Typescript::default(), &out_path)
        .expect("specta export should succeed");

    let contents = std::fs::read_to_string(&out_path).expect("bindings.ts should have been written");
    assert!(!contents.trim().is_empty(), "bindings.ts should not be empty");
    assert!(contents.contains("get_app_info"), "bindings.ts should contain the get_app_info command");

    let _ = std::fs::remove_dir_all(dir);
}
