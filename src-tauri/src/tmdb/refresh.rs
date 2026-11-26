//! Fetching one title and storing the result. The database is only locked
//! before and after the network requests, never during them.

use crate::db::metadata::{self as store, Target};
use crate::error::{AppError, AppResult};
use crate::state::{read, unix_now, AppState};

use super::cache::ImageCache;
use super::fetch::{self, Api};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// From the background queue: skipped unless the title still needs it.
    Auto,
    /// The user asked for this title to be refreshed.
    Manual,
    /// The user chose the TMDB entry. It is locked in once fetched.
    Fix(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The library changed.
    Stored,
    /// TMDB has nothing for this title. Existing metadata is left alone.
    NoMatch,
    /// Not needed, or the title changed while it was being fetched.
    Skipped,
}

/// Nothing is written unless every request succeeds.
pub fn refresh(state: &AppState, api: &impl Api, title_id: i64, mode: Mode) -> AppResult<Outcome> {
    // Keeps the image cache from being cleared until the paths are stored.
    let _images = read(&state.image_gate);
    // Taken before the files are read, so later additions count as new.
    let started = unix_now();
    let target = {
        let db = state.db();
        if mode == Mode::Auto && !store::needs_fetch(&db, title_id, started)? {
            return Ok(Outcome::Skipped);
        }
        store::target(&db, title_id)?.ok_or(AppError::NotFound)?
    };

    let result = fetch_and_store(state, api, &target, mode, started);
    if let Err(error) = &result {
        // Asking again soon would only get the same answer.
        if mode == Mode::Auto && is_permanent(error) {
            store::mark_attempted(&state.db(), &target, started)?;
        }
    }
    result
}

fn is_permanent(error: &AppError) -> bool {
    match error {
        AppError::Tmdb(status) => (400..500).contains(status) && *status != 429,
        AppError::BadData(_) => true,
        _ => false,
    }
}

fn fetch_and_store(
    state: &AppState,
    api: &impl Api,
    target: &Target,
    mode: Mode,
    started: i64,
) -> AppResult<Outcome> {
    let known = match mode {
        Mode::Fix(tmdb_id) => Some(tmdb_id),
        Mode::Auto | Mode::Manual => target.tmdb_id,
    };
    let tmdb_id = match known {
        Some(tmdb_id) => tmdb_id,
        None => {
            match fetch::find_match(api, target.kind, &target.guessed_title, target.guessed_year)? {
                Some(tmdb_id) => tmdb_id,
                None => {
                    store::mark_attempted(&state.db(), target, started)?;
                    return Ok(Outcome::NoMatch);
                }
            }
        }
    };

    let images = ImageCache::new(&state.data_dir);
    let fetched = fetch::fetch(api, &images, target.kind, tmdb_id, &target.episodes)?;
    // An incomplete result stays stale, so it is tried again.
    let fetched_at = if fetched.is_complete() { started } else { 0 };
    let lock_match = matches!(mode, Mode::Fix(_));
    let stored = state.with_db(|db| store::save(db, target, &fetched, fetched_at, lock_match))?;
    Ok(if stored {
        Outcome::Stored
    } else {
        Outcome::Skipped
    })
}

#[cfg(test)]
pub mod testing;
#[cfg(test)]
mod tests;
