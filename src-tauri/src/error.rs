// Temporary: first used by the Phase 1 commands.
#![allow(dead_code)]

use serde::{Serialize, Serializer};

/// The single error type of the app. It reaches the UI as a friendly message.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Something went wrong: {0}")]
    Tauri(#[from] tauri::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
