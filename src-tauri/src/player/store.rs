//! What playback writes to the database. Each function reports whether the
//! library on screen is now out of date.

use rusqlite::{params, Connection};

use super::tracker::Save;
use crate::error::AppResult;
use crate::progress;

/// Stores a position. True if this is what made the file watched.
pub fn save(conn: &Connection, save: &Save, now: i64) -> AppResult<bool> {
    let newly_watched = save.marks_watched() && !progress::get(conn, save.file_id)?.watched;
    progress::save(conn, save.file_id, save.position, save.duration, now)?;
    Ok(newly_watched)
}

/// The end of a file: watched, with nothing left to resume.
pub fn finish(conn: &Connection, file_id: i64, duration: Option<f64>, now: i64) -> AppResult<()> {
    // The first call makes sure the duration is stored.
    progress::save(conn, file_id, 0.0, duration, now)?;
    progress::set_watched(conn, file_id, true, now)
}

pub fn set_dimensions(conn: &Connection, file_id: i64, width: i64, height: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE files SET width = ?2, height = ?3 WHERE id = ?1",
        params![file_id, width, height],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn library() -> Connection {
        let conn = db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO titles (id, kind, name_key, guessed_title)
               VALUES (1, 'movie', 'heat', 'Heat');
             INSERT INTO files (id, path, title_id, size, mtime, added_at)
               VALUES (10, '/m/heat.mkv', 1, 5, 0, 1);",
        )
        .unwrap();
        conn
    }

    fn at(position: f64) -> Save {
        Save {
            file_id: 10,
            position,
            duration: Some(1000.0),
        }
    }

    #[test]
    fn reports_only_the_save_that_marks_watched() {
        let conn = library();
        assert!(!save(&conn, &at(500.0), 1).unwrap());
        assert!(save(&conn, &at(950.0), 2).unwrap());
        assert!(!save(&conn, &at(960.0), 3).unwrap());
    }

    #[test]
    fn finishing_clears_the_position_and_keeps_the_duration() {
        let conn = library();
        save(&conn, &at(500.0), 1).unwrap();
        finish(&conn, 10, Some(1000.0), 2).unwrap();
        let stored = progress::get(&conn, 10).unwrap();
        assert!(stored.watched);
        assert!(stored.position.abs() < f64::EPSILON);
        assert_eq!(stored.duration, Some(1000.0));
    }

    #[test]
    fn stores_the_video_size() {
        let conn = library();
        set_dimensions(&conn, 10, 1920, 800).unwrap();
        let size: (i64, i64) = conn
            .query_row("SELECT width, height FROM files WHERE id = 10", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(size, (1920, 800));
    }
}
