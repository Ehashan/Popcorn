//! Watch progress: the rules (pure) and how they are stored.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;

/// Below this many seconds there is nothing worth resuming.
const RESUME_AFTER_SECS: f64 = 60.0;
/// Past this share of the duration a file counts as watched.
const WATCHED_RATIO: f64 = 0.9;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Progress {
    pub position: f64,
    pub duration: Option<f64>,
    pub watched: bool,
    /// 0 when there has never been any activity.
    pub updated_at: i64,
}

pub fn reaches_watched(position: f64, duration: Option<f64>) -> bool {
    duration.is_some_and(|d| d > 0.0 && position >= d * WATCHED_RATIO)
}

/// Where "Resume" should start, if it should be offered at all.
pub fn resume_position(position: f64, duration: Option<f64>) -> Option<f64> {
    let duration = duration.filter(|d| *d > 0.0)?;
    (position > RESUME_AFTER_SECS && position < duration * WATCHED_RATIO).then_some(position)
}

/// Picks the episode that "Play next" should start. `episodes` must be sorted
/// by season and episode. Returns an index into it.
pub fn next_up(episodes: &[(u32, Progress)]) -> Option<usize> {
    let latest = episodes
        .iter()
        .enumerate()
        .filter(|(_, (_, p))| p.updated_at > 0)
        .max_by_key(|(index, (_, p))| (p.updated_at, *index));

    let Some((index, (_, progress))) = latest else {
        // Nothing watched yet: start with the first regular episode.
        return episodes
            .iter()
            .position(|(season, p)| *season > 0 && !p.watched)
            .or_else(|| episodes.iter().position(|(_, p)| !p.watched));
    };
    if !progress.watched || resume_position(progress.position, progress.duration).is_some() {
        return Some(index);
    }
    episodes
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, (_, p))| !p.watched)
        .map(|(next, _)| next)
}

pub fn get(conn: &Connection, file_id: i64) -> AppResult<Progress> {
    Ok(conn
        .query_row(
            "SELECT position, duration, watched, updated_at FROM progress WHERE file_id = ?1",
            [file_id],
            |row| {
                Ok(Progress {
                    position: row.get(0)?,
                    duration: row.get(1)?,
                    watched: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

/// Saves a playback position. A file stays watched once it has been watched.
pub fn save(
    conn: &Connection,
    file_id: i64,
    position: f64,
    duration: Option<f64>,
    now: i64,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO progress (file_id, position, duration, watched, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (file_id) DO UPDATE SET
           position = excluded.position,
           duration = COALESCE(excluded.duration, progress.duration),
           watched = MAX(progress.watched, excluded.watched),
           updated_at = excluded.updated_at",
        params![
            file_id,
            position.max(0.0),
            duration,
            reaches_watched(position, duration),
            now
        ],
    )?;
    Ok(())
}

/// The manual toggle, and what happens at the end of a file.
pub fn set_watched(conn: &Connection, file_id: i64, watched: bool, now: i64) -> AppResult<()> {
    conn.execute(
        "INSERT INTO progress (file_id, position, watched, updated_at)
         VALUES (?1, 0, ?2, ?3)
         ON CONFLICT (file_id) DO UPDATE SET
           position = 0, watched = excluded.watched, updated_at = excluded.updated_at",
        params![file_id, watched, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(updated_at: i64) -> Progress {
        Progress {
            watched: true,
            updated_at,
            ..Progress::default()
        }
    }

    fn partway(updated_at: i64) -> Progress {
        Progress {
            position: 600.0,
            duration: Some(3000.0),
            watched: false,
            updated_at,
        }
    }

    #[test]
    fn resume_window() {
        assert_eq!(resume_position(60.0, Some(1000.0)), None);
        assert_eq!(resume_position(61.0, Some(1000.0)), Some(61.0));
        assert_eq!(resume_position(900.0, Some(1000.0)), None);
        assert_eq!(resume_position(500.0, None), None);
        assert!(reaches_watched(900.0, Some(1000.0)));
        assert!(!reaches_watched(899.0, Some(1000.0)));
    }

    #[test]
    fn next_up_starts_after_specials() {
        let new = Progress::default();
        assert_eq!(next_up(&[(0, new), (1, new), (1, new)]), Some(1));
        assert_eq!(next_up(&[(0, new)]), Some(0));
        assert_eq!(next_up(&[]), None);
    }

    #[test]
    fn next_up_follows_the_latest_activity() {
        let new = Progress::default();
        assert_eq!(next_up(&[(1, seen(1)), (1, partway(2)), (1, new)]), Some(1));
        assert_eq!(next_up(&[(1, seen(1)), (1, seen(2)), (2, new)]), Some(2));
        assert_eq!(next_up(&[(1, new), (1, seen(5)), (1, new)]), Some(2));
        assert_eq!(next_up(&[(1, seen(1)), (1, seen(2))]), None);
    }
}
