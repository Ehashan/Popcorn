/// Every app command must be listed here so that it gets its own permission,
/// which `capabilities/default.json` then has to grant explicitly.
const COMMANDS: &[&str] = &["app_info"];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to run tauri-build");
}
