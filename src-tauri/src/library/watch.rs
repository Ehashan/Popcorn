//! Watches the media folder and reports, debounced, when videos may have changed.

use std::path::Path;
use std::time::Duration;

use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use super::parse;
use crate::error::AppResult;

const DEBOUNCE: Duration = Duration::from_secs(2);

/// Dropping this stops the watcher.
pub type Watcher = Debouncer<RecommendedWatcher, RecommendedCache>;

pub fn watch(root: &Path, on_change: impl Fn() + Send + 'static) -> AppResult<Watcher> {
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        let relevant = match result {
            Ok(events) => events.iter().any(|event| {
                !matches!(event.kind, EventKind::Access(_))
                    && event.paths.iter().any(|path| could_matter(path))
            }),
            // Events may have been dropped, so look again to be safe.
            Err(_) => true,
        };
        if relevant {
            on_change();
        }
    })?;
    debouncer.watch(root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

/// Videos, and anything that might be a folder (which may be gone by now).
fn could_matter(path: &Path) -> bool {
    parse::is_video(path) || path.is_dir() || path.extension().is_none() || !path.exists()
}
