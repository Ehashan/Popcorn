//! The state shared by commands and background jobs.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::error::AppResult;
use crate::library::watch::Watcher;
use crate::tmdb::queue::JobQueue;

pub struct AppState {
    db: Mutex<Connection>,
    /// `~/Library/Application Support/Popcorn`
    pub data_dir: PathBuf,
    /// Replaced whenever the media folder changes.
    pub watcher: Mutex<Option<Watcher>>,
    /// Held for the length of a scan, so scans never overlap.
    pub scan_gate: Mutex<()>,
    /// Titles waiting for metadata.
    pub metadata_queue: JobQueue,
    /// Read while fetching a title, written to clear the image cache.
    pub image_gate: RwLock<()>,
}

impl AppState {
    pub fn new(db: Connection, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            watcher: Mutex::new(None),
            scan_gate: Mutex::new(()),
            metadata_queue: JobQueue::default(),
            image_gate: RwLock::new(()),
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

pub fn read<T>(gate: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    gate.read().unwrap_or_else(PoisonError::into_inner)
}

pub fn write<T>(gate: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    gate.write().unwrap_or_else(PoisonError::into_inner)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
