//! The queries behind fetching and storing TMDB metadata.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::error::AppResult;
use crate::library::group::TitleKind;
use crate::tmdb::fetch::{Fetched, Image};
use crate::tmdb::metadata::Metadata;

const MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

/// A title as it was when its fetch began.
#[derive(Debug)]
pub struct Target {
    pub id: i64,
    pub kind: TitleKind,
    name_key: String,
    pub guessed_title: String,
    pub guessed_year: Option<i64>,
    pub tmdb_id: Option<i64>,
    match_locked: bool,
    /// The (season, episode) pairs on disk.
    pub episodes: Vec<(u32, u32)>,
}

fn kind_of(kind: &str) -> TitleKind {
    if kind == TitleKind::Show.as_str() {
        TitleKind::Show
    } else {
        TitleKind::Movie
    }
}

pub fn title_kind(conn: &Connection, id: i64) -> AppResult<Option<TitleKind>> {
    Ok(conn
        .query_row("SELECT kind FROM titles WHERE id = ?1", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .map(|kind| kind_of(&kind)))
}

pub fn target(conn: &Connection, id: i64) -> AppResult<Option<Target>> {
    let target = conn
        .query_row(
            "SELECT kind, name_key, guessed_title, guessed_year, tmdb_id, match_locked
             FROM titles WHERE id = ?1",
            [id],
            |row| {
                Ok(Target {
                    id,
                    kind: kind_of(&row.get::<_, String>(0)?),
                    name_key: row.get(1)?,
                    guessed_title: row.get(2)?,
                    guessed_year: row.get(3)?,
                    tmdb_id: row.get(4)?,
                    match_locked: row.get(5)?,
                    episodes: Vec::new(),
                })
            },
        )
        .optional()?;
    let Some(mut target) = target else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT DISTINCT season, episode FROM files
         WHERE title_id = ?1 AND missing = 0 AND season IS NOT NULL AND episode IS NOT NULL
         ORDER BY season, episode",
    )?;
    let rows = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    target.episodes = rows.collect::<Result<_, _>>()?;
    Ok(Some(target))
}

/// Titles with files on disk that were never fetched or have gone stale, and
/// matched shows that gained an episode since. Newest additions come first.
fn stale_titles(conn: &Connection, now: i64, only: Option<i64>) -> AppResult<Vec<i64>> {
    let mut statement = conn.prepare(
        "SELECT t.id FROM titles t
         JOIN files f ON f.title_id = t.id AND f.missing = 0
         WHERE ?2 IS NULL OR t.id = ?2
         GROUP BY t.id
         HAVING t.fetched_at IS NULL
             OR t.fetched_at < ?1
             OR (t.kind = 'show' AND t.metadata_json IS NOT NULL AND MAX(
                   f.added_at >= t.fetched_at AND NOT EXISTS (
                     SELECT 1 FROM episode_meta m WHERE m.title_id = f.title_id
                       AND m.season = f.season AND m.episode = f.episode)))
         ORDER BY MAX(f.added_at) DESC, t.id",
    )?;
    let rows = statement.query_map(params![now - MAX_AGE_SECS, only], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn needing_fetch(conn: &Connection, now: i64) -> AppResult<Vec<i64>> {
    stale_titles(conn, now, None)
}

pub fn needs_fetch(conn: &Connection, id: i64, now: i64) -> AppResult<bool> {
    Ok(!stale_titles(conn, now, Some(id))?.is_empty())
}

/// Records that a title was looked up, without touching its metadata. Does
/// nothing if its match changed in the meantime.
pub fn mark_attempted(conn: &Connection, target: &Target, now: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE titles SET fetched_at = ?1
         WHERE id = ?2 AND kind = ?3 AND name_key = ?4
           AND tmdb_id IS ?5 AND match_locked = ?6",
        params![
            now,
            target.id,
            target.kind.as_str(),
            target.name_key,
            target.tmdb_id,
            target.match_locked
        ],
    )?;
    Ok(())
}

struct Previous {
    tmdb_id: Option<i64>,
    match_locked: bool,
    metadata: Option<Metadata>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

/// `None` if the title is gone. The name is checked too, because SQLite may
/// give a deleted title's id to a new one.
fn previous(tx: &Transaction<'_>, target: &Target) -> AppResult<Option<Previous>> {
    Ok(tx
        .query_row(
            "SELECT tmdb_id, match_locked, metadata_json, poster_path, backdrop_path
             FROM titles WHERE id = ?1 AND kind = ?2 AND name_key = ?3",
            params![target.id, target.kind.as_str(), target.name_key],
            |row| {
                Ok(Previous {
                    tmdb_id: row.get(0)?,
                    match_locked: row.get(1)?,
                    metadata: row
                        .get::<_, Option<String>>(2)?
                        .and_then(|json| serde_json::from_str(&json).ok()),
                    poster_path: row.get(3)?,
                    backdrop_path: row.get(4)?,
                })
            },
        )
        .optional()?)
}

fn image_path(new: &Image, old: Option<String>) -> Option<String> {
    match new {
        Image::Cached(path) => Some(path.clone()),
        Image::Absent => None,
        Image::Unavailable => old,
    }
}

/// Stores a fetched match in one transaction. When the match is the same as
/// before, images that could not be fetched keep their old copies; otherwise
/// everything about the old match is replaced.
///
/// `lock_match` is for the user's own choice, which always wins. Any other
/// result is discarded (returning false) if the match changed during the fetch.
pub fn save(
    conn: &mut Connection,
    target: &Target,
    fetched: &Fetched,
    fetched_at: i64,
    lock_match: bool,
) -> AppResult<bool> {
    let tx = conn.transaction()?;
    let Some(old) = previous(&tx, target)? else {
        return Ok(false);
    };
    let unchanged = old.tmdb_id == target.tmdb_id && old.match_locked == target.match_locked;
    if !lock_match && !unchanged {
        return Ok(false);
    }
    let old_metadata = old
        .metadata
        .filter(|_| old.tmdb_id == Some(fetched.tmdb_id));
    let same_match = old_metadata.is_some();

    let mut metadata = fetched.metadata.clone();
    if let Some(old_metadata) = old_metadata {
        for member in &mut metadata.cast {
            if member.profile_path.is_none() {
                member.profile_path = old_metadata
                    .cast
                    .iter()
                    .find(|m| m.name == member.name)
                    .and_then(|m| m.profile_path.clone());
            }
        }
    }
    let poster = image_path(&fetched.poster, old.poster_path.filter(|_| same_match));
    let backdrop = image_path(&fetched.backdrop, old.backdrop_path.filter(|_| same_match));

    tx.execute(
        "UPDATE titles SET tmdb_id = ?1, match_locked = MAX(match_locked, ?2),
           metadata_json = ?3, poster_path = ?4, backdrop_path = ?5, fetched_at = ?6
         WHERE id = ?7",
        params![
            fetched.tmdb_id,
            lock_match,
            serde_json::to_string(&metadata)?,
            poster,
            backdrop,
            fetched_at,
            target.id
        ],
    )?;

    if !same_match {
        tx.execute("DELETE FROM episode_meta WHERE title_id = ?1", [target.id])?;
    }
    for episode in &fetched.episodes {
        tx.execute(
            "INSERT INTO episode_meta (title_id, season, episode, name, overview, still_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (title_id, season, episode) DO UPDATE SET
               name = excluded.name, overview = excluded.overview,
               still_path = COALESCE(excluded.still_path, episode_meta.still_path)",
            params![
                target.id,
                episode.season,
                episode.episode,
                episode.name,
                episode.overview,
                episode.still_path
            ],
        )?;
    }
    tx.commit()?;
    Ok(true)
}

/// Lets titles that had no match be searched again.
pub fn retry_unmatched(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "UPDATE titles SET fetched_at = NULL WHERE metadata_json IS NULL",
        [],
    )?;
    Ok(())
}

/// Makes every title due. It stays due until a fetch succeeds.
pub fn mark_all_stale(conn: &Connection) -> AppResult<()> {
    conn.execute("UPDATE titles SET fetched_at = 0", [])?;
    Ok(())
}

/// Removes every reference to a cached image and makes the matched titles due.
/// The rest of the metadata stays.
pub fn forget_images(conn: &mut Connection) -> AppResult<()> {
    let tx = conn.transaction()?;
    let stored = {
        let mut statement =
            tx.prepare("SELECT id, metadata_json FROM titles WHERE metadata_json IS NOT NULL")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<(i64, String)>, _>>()?
    };
    for (id, json) in stored {
        let Ok(mut metadata) = serde_json::from_str::<Metadata>(&json) else {
            continue;
        };
        for member in &mut metadata.cast {
            member.profile_path = None;
        }
        tx.execute(
            "UPDATE titles SET metadata_json = ?1, fetched_at = 0 WHERE id = ?2",
            params![serde_json::to_string(&metadata)?, id],
        )?;
    }
    tx.execute(
        "UPDATE titles SET poster_path = NULL, backdrop_path = NULL",
        [],
    )?;
    tx.execute("UPDATE episode_meta SET still_path = NULL", [])?;
    tx.commit()?;
    Ok(())
}
