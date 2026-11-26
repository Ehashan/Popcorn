use serde::{Serialize, Serializer};

/// The single error type of the app. It reaches the UI as a friendly message.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Something went wrong: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("The library database reported a problem: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("A file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not watch the media folder: {0}")]
    Watch(#[from] notify_debouncer_full::notify::Error),
    #[error("Received data that could not be read")]
    BadData(#[from] serde_json::Error),
    #[error("The media folder is not available. Is the drive connected?")]
    FolderUnavailable,
    #[error("Could not reach TMDB. Check your internet connection.")]
    Offline,
    #[error("TMDB did not accept that key. Check that you copied all of it.")]
    InvalidTmdbKey,
    #[error("Add your TMDB API key in Settings first.")]
    MissingTmdbKey,
    #[error("TMDB returned an error (HTTP {0}). Try again later.")]
    Tmdb(u16),
    #[error("That item is no longer in the library.")]
    NotFound,
    #[error("Playback problem: {0}")]
    Player(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
