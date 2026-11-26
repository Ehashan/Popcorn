//! The state shared by commands and background jobs.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::error::AppResult;
use crate::library::watch::Watcher;

pub struct AppState {
    db: Mutex<Connection>,
    /// `~/Library/Application Support/Popcorn`
    pub data_dir: PathBuf,
    /// Replaced whenever the media folder changes.
    pub watcher: Mutex<Option<Watcher>>,
    /// Held for the length of a scan, so scans never overlap.
    pub scan_gate: Mutex<()>,
}

impl AppState {
    pub fn new(db: Connection, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            watcher: Mutex::new(None),
            scan_gate: Mutex::new(()),
        }
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        lock(&self.db)
    }

    pub fn with_db<T>(&self, run: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        run(&mut self.db())
    }
}

/// A panic aborts the release build, so a poisoned lock is safe to reuse.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
