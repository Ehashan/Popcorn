mod commands;
mod error;
mod logger;

pub fn run() {
    logger::init();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())
        .expect("failed to start Popcorn");
}
