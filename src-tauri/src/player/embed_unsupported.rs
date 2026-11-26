//! Platforms without a video surface yet. `embed_macos.rs` has the real one.

use std::sync::Arc;

use tauri::AppHandle;

use super::mpv::Mpv;
use crate::error::{AppError, AppResult};

pub type KeyHandler = Arc<dyn Fn(u16, &str, usize) + Send + Sync>;

#[derive(Clone)]
pub struct VideoSurface;

fn unsupported() -> AppError {
    AppError::Player("video is not supported on this system yet".into())
}

impl VideoSurface {
    pub fn embed(_: &AppHandle, _: &Arc<Mpv>) -> AppResult<Self> {
        Err(unsupported())
    }

    pub fn open_window(_: &AppHandle, _: &Arc<Mpv>, _: KeyHandler) -> AppResult<Self> {
        Err(unsupported())
    }

    pub fn is_embedded(&self) -> bool {
        false
    }

    pub fn set_scale(&self, _: &AppHandle, _: f64) {}

    pub fn is_window_fullscreen(&self) -> bool {
        false
    }

    pub fn toggle_window_fullscreen(&self, _: &AppHandle) -> AppResult<bool> {
        Err(unsupported())
    }

    pub fn set_window_fullscreen(&self, _: &AppHandle, _: bool) -> AppResult<()> {
        Err(unsupported())
    }

    pub fn close(self, _: &AppHandle) {}

    #[cfg(debug_assertions)]
    pub fn press_key(&self, _: &AppHandle, _: u16, _: &'static std::ffi::CStr) {}

    #[cfg(debug_assertions)]
    pub fn describe(&self, _: &AppHandle) -> String {
        String::new()
    }
}

pub struct WakeLock;

impl WakeLock {
    pub fn new() -> Self {
        Self
    }
}
