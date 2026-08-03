use std::path::PathBuf;

fn main() {
    stage_helper();
    tauri_build::build()
}

/// Copy the helper next to where it will be bundled from.
///
/// A resource path containing `..` is emitted by Tauri under an `_up_`
/// directory, so the helper landed at `_up_\target\release\threshold-helper.exe`
/// while the app looked for it beside its own executable. Staging it into
/// `src-tauri/resources/` keeps the bundled path flat and predictable.
fn stage_helper() {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace_target = manifest
        .parent()
        .map(|root| root.join("target").join(&profile))
        .unwrap_or_default();

    let source = workspace_target.join("threshold-helper.exe");
    let dest_dir = manifest.join("resources");
    let dest = dest_dir.join("threshold-helper.exe");

    println!("cargo:rerun-if-changed={}", source.display());

    if !source.exists() {
        // The helper is a sibling binary; on a from-scratch build it may not
        // exist yet. Bundling would fail loudly, and `cargo build` of the
        // workspace produces it, so a placeholder keeps the config valid.
        if !dest.exists() {
            let _ = std::fs::create_dir_all(&dest_dir);
            let _ = std::fs::write(&dest, b"");
            println!("cargo:warning=threshold-helper.exe not built yet; staged a placeholder. Run `cargo build -p threshold-helper` and rebuild.");
        }
        return;
    }

    let _ = std::fs::create_dir_all(&dest_dir);
    if let Err(err) = std::fs::copy(&source, &dest) {
        println!("cargo:warning=could not stage threshold-helper.exe: {err}");
    }
}
