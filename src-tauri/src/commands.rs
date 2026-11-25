//! All `#[tauri::command]` functions.

use serde::Serialize;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
    }
}
