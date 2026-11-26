//! Background work: scanning, watching and fetching metadata. This is the glue
//! between the Tauri-free code and the app.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::metadata;
use crate::error::{AppError, AppResult};
use crate::library::{scan, watch};
use crate::settings;
use crate::state::{lock, unix_now, AppState};
use crate::tmdb::client::Client;
use crate::tmdb::queue::{Job, Throttle};
use crate::tmdb::refresh::{self, Mode, Outcome};

pub const LIBRARY_UPDATED: &str = "library://updated";
const SCAN_PROGRESS: &str = "scan://progress";
const PROGRESS_EVERY: usize = 25;
/// Each worker makes one request at a time.
const METADATA_WORKERS: usize = 4;

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
        queue_metadata(&app);
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

/// Starts the threads that work through the metadata queue. Call once.
pub fn start_metadata_workers(app: &AppHandle) {
    let throttle = Arc::new(Mutex::new(Throttle::default()));
    for _ in 0..METADATA_WORKERS {
        let app = app.clone();
        let throttle = Arc::clone(&throttle);
        thread::spawn(move || metadata_worker(&app, &throttle));
    }
}

/// Queues every title with missing or stale metadata, if there is a TMDB key.
/// Failures are only logged: the library works without metadata.
pub fn queue_metadata(app: &AppHandle) {
    let state = app.state::<AppState>();
    let due = || -> AppResult<Vec<i64>> {
        let db = state.db();
        if settings::get(&db, settings::TMDB_KEY)?.is_none() {
            return Ok(Vec::new());
        }
        metadata::needing_fetch(&db, unix_now())
    };
    match due() {
        Ok(title_ids) => state.metadata_queue.push(title_ids),
        Err(error) => log::warn!("could not queue metadata: {error}"),
    }
}

fn metadata_worker(app: &AppHandle, throttle: &Mutex<Throttle>) {
    let state = app.state::<AppState>();
    // Kept between jobs, so that connections are reused.
    let mut client = None;
    loop {
        let job = state.metadata_queue.next();
        let changed = run_metadata_job(&state, &mut client, job);
        // Held across both steps, so the last job's update cannot be lost.
        let mut throttle = lock(throttle);
        let idle = state.metadata_queue.finish(job);
        if throttle.record(changed, idle, Instant::now()) {
            notify_library_updated(app);
        }
    }
}

/// True if the library changed.
fn run_metadata_job(state: &AppState, client: &mut Option<(String, Client)>, job: Job) -> bool {
    match fetch_metadata(state, client, job.title_id) {
        Ok(outcome) => outcome == Outcome::Stored,
        // Deleted while it waited.
        Err(AppError::NotFound) => false,
        Err(error) => {
            log::warn!("could not fetch metadata: {error}");
            // The rest would fail the same way. A later scan queues them again.
            if matches!(
                error,
                AppError::Offline | AppError::InvalidTmdbKey | AppError::MissingTmdbKey
            ) {
                state.metadata_queue.abort(job);
            }
            false
        }
    }
}

fn fetch_metadata(
    state: &AppState,
    cached: &mut Option<(String, Client)>,
    title_id: i64,
) -> AppResult<Outcome> {
    // A statement of its own, so the database is unlocked again before the fetch.
    let key = settings::get(&state.db(), settings::TMDB_KEY)?.ok_or(AppError::MissingTmdbKey)?;
    let client = match cached.take() {
        Some(client) if client.0 == key => client,
        _ => (key.clone(), Client::new(&key)),
    };
    let (_, client) = cached.insert(client);
    refresh::refresh(state, client, title_id, Mode::Auto)
}
