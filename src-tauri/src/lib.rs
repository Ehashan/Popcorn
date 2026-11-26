// Temporary while TMDB and the player are being built.
#![allow(dead_code)]

mod commands;
mod db;
mod error;
mod jobs;
mod library;
mod logger;
mod progress;
mod settings;
mod state;
mod tmdb;

use tauri::Manager;

use crate::state::AppState;

pub fn run() {
    logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().data_dir()?.join("Popcorn");
            std::fs::create_dir_all(data_dir.join("images"))?;
            let db = db::open(&data_dir.join("popcorn.db"))?;
            app.manage(AppState::new(db, data_dir));
            if let Err(error) = jobs::start_watcher(app.handle()) {
                log::warn!("not watching the media folder: {error}");
            }
            jobs::start_scan(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::pick_media_folder,
            commands::finish_onboarding,
            commands::set_tmdb_key,
            commands::test_tmdb_key,
            commands::rescan,
            commands::list_titles,
            commands::get_title,
            commands::continue_watching,
            commands::search_library,
            commands::set_watched,
            commands::set_title_watched,
            commands::reveal_file,
            commands::open_external,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Popcorn");
}
