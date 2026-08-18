fn main() {
    let attributes = tauri_build::Attributes::new();
    // Shadowed (not mutated) so the binding stays plain `let` on non-Windows
    // targets — a `mut` here was only ever reassigned inside this cfg(windows)
    // block, so `-D warnings` clippy on Linux CI (where this block compiles
    // out entirely) flagged it as unused_mut even though it's genuinely
    // needed on Windows.
    #[cfg(windows)]
    let attributes = {
        // tauri-build's default manifest embedding uses cargo:rustc-link-arg-bins,
        // which only links the manifest into [[bin]] targets — never into test
        // binaries (cargo test, internal #[test] modules, examples). Without the
        // manifest, those binaries fail to launch on Windows with
        // STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). Embedding it ourselves via
        // plain cargo:rustc-link-arg applies to every binary target instead.
        // See: https://github.com/tauri-apps/tauri/issues/13419
        add_manifest();
        attributes.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    };
    tauri_build::try_build(attributes).unwrap();
}

#[cfg(windows)]
fn add_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

    let manifest = std::env::current_dir().unwrap().join(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={}", manifest.display());
    // Embed the Windows application manifest file into every binary target.
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.to_str().unwrap()
    );
    // Turn linker warnings into errors.
    println!("cargo:rustc-link-arg=/WX");
}
