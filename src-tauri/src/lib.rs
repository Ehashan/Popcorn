// Temporary while TMDB and the player are being built.
#![allow(dead_code)]

mod commands;
mod db;
mod error;
mod jobs;
mod library;
mod logger;
mod player;
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
        .manage(player::PlayerHost::default())
        .on_window_event(player::on_window_event)
        .setup(|app| {
            let data_dir = app.path().data_dir()?.join("Popcorn");
            #[cfg(debug_assertions)]
            let data_dir = player::smoke::data_dir().unwrap_or(data_dir);
            std::fs::create_dir_all(data_dir.join("images"))?;
            let db = db::open(&data_dir.join("popcorn.db"))?;
            app.manage(AppState::new(db, data_dir));
            if let Err(error) = jobs::start_watcher(app.handle()) {
                log::warn!("not watching the media folder: {error}");
            }
            jobs::start_scan(app.handle());
            #[cfg(debug_assertions)]
            player::smoke::start(app.handle());
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
            player::commands::player_open,
            player::commands::player_close,
            player::commands::player_toggle_pause,
            player::commands::player_seek,
            player::commands::player_set_volume,
            player::commands::player_toggle_mute,
            player::commands::player_set_audio,
            player::commands::player_set_subtitle,
            player::commands::player_add_subtitle,
            player::commands::player_toggle_fullscreen,
            player::commands::player_exit_fullscreen,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start Popcorn")
        .run(|app, event| player::on_run_event(app, &event));
}
