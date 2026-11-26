//! Background work: scanning and watching. This is the glue between the
//! Tauri-free library code and the app.

use std::path::PathBuf;
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppResult;
use crate::library::{scan, watch};
use crate::settings;
use crate::state::{lock, unix_now, AppState};

pub const LIBRARY_UPDATED: &str = "library://updated";
const SCAN_PROGRESS: &str = "scan://progress";
const PROGRESS_EVERY: usize = 25;

#[derive(Clone, Serialize)]
struct ScanProgress {
    found: usize,
    done: bool,
    error: Option<String>,
}

pub fn media_folder(state: &AppState) -> AppResult<Option<PathBuf>> {
    Ok(settings::get(&state.db(), settings::MEDIA_FOLDER)?.map(PathBuf::from))
}

pub fn notify_library_updated(app: &AppHandle) {
    emit(app, LIBRARY_UPDATED, ());
}

fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(error) = app.emit(event, payload) {
        log::warn!("could not emit {event}: {error}");
    }
}

/// Scans in the background and reports through events.
pub fn start_scan(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let mut found = 0;
        let result = run_scan(&app, &mut found);
        let error = result.err().map(|error| {
            log::warn!("scan failed: {error}");
            error.to_string()
        });
        emit(
            &app,
            SCAN_PROGRESS,
            ScanProgress {
                found,
                done: true,
                error,
            },
        );
    });
}

fn run_scan(app: &AppHandle, found: &mut usize) -> AppResult<()> {
    let state = app.state::<AppState>();
    let _gate = lock(&state.scan_gate);
    let Some(root) = media_folder(&state)? else {
        return Ok(());
    };
    // The database is not locked while walking, so the UI stays responsive.
    let walk = scan::walk(&root, |count| {
        *found = count;
        if count % PROGRESS_EVERY == 0 {
            let progress = ScanProgress {
                found: count,
                done: false,
                error: None,
            };
            emit(app, SCAN_PROGRESS, progress);
        }
    })?;
    // The folder may have been changed while we were walking the old one.
    if media_folder(&state)?.as_ref() != Some(&root) {
        return Ok(());
    }
    let summary = state.with_db(|db| scan::apply(db, walk, unix_now()))?;
    log::info!("scan finished: {summary:?}");
    if summary.changed() {
        notify_library_updated(app);
    }
    Ok(())
}

/// (Re)starts the watcher on the current media folder.
pub fn start_watcher(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let watcher = match media_folder(&state)? {
        Some(root) if root.is_dir() => {
            let app = app.clone();
            Some(watch::watch(&root, move || start_scan(&app))?)
        }
        _ => None,
    };
    *lock(&state.watcher) = watcher;
    Ok(())
}
