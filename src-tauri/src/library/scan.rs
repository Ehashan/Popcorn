//! Walks the media folder and brings the database in line with it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, Transaction};
use walkdir::{DirEntry, WalkDir};

use super::group::{group, TitleGroup};
use super::parse;
use crate::error::{AppError, AppResult};

#[derive(Debug)]
pub struct FoundFile {
    pub path: String,
    /// The path starting at the media folder's own name, as the parser wants it.
    pub parse_path: PathBuf,
    pub size: i64,
    pub mtime: i64,
}

#[derive(Debug, Default)]
pub struct Walk {
    pub files: Vec<FoundFile>,
    /// Places we could not read. Files below them are not marked missing.
    pub unreadable: Vec<PathBuf>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanSummary {
    pub added: usize,
    pub updated: usize,
    pub missing: usize,
}

impl ScanSummary {
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.missing > 0
    }
}

/// Fails if the folder itself cannot be read (say, an unplugged drive), so that
/// the whole library is not marked missing by mistake.
pub fn walk(root: &Path, mut on_found: impl FnMut(usize)) -> AppResult<Walk> {
    std::fs::read_dir(root).map_err(|_| AppError::FolderUnavailable)?;
    let parent = root.parent().unwrap_or(root);
    let mut result = Walk::default();

    let visible = |entry: &DirEntry| {
        entry.depth() == 0 || !entry.file_name().to_string_lossy().starts_with('.')
    };
    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_entry(visible)
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                log::warn!("skipping unreadable entry: {error}");
                result
                    .unreadable
                    .extend(error.path().map(Path::to_path_buf));
                continue;
            }
        };
        if !entry.file_type().is_file() || !parse::is_video(entry.path()) {
            continue;
        }
        let (Some(path), Ok(meta)) = (entry.path().to_str(), entry.metadata()) else {
            continue;
        };
        if parse::is_ignored(&entry.file_name().to_string_lossy(), meta.len()) {
            continue;
        }
        result.files.push(FoundFile {
            path: path.to_string(),
            parse_path: entry
                .path()
                .strip_prefix(parent)
                .unwrap_or(entry.path())
                .to_path_buf(),
            size: i64::try_from(meta.len()).unwrap_or(i64::MAX),
            mtime: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0)),
        });
        on_found(result.files.len());
    }
    // A stable order, so the same file always names a new title.
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

struct KnownFile {
    id: i64,
    size: i64,
    mtime: i64,
    missing: bool,
}

/// Diffs what is on disk against the database by path, size and mtime.
pub fn apply(conn: &mut Connection, walk: Walk, now: i64) -> AppResult<ScanSummary> {
    let tx = conn.transaction()?;
    let mut known = known_files(&tx)?;
    let mut summary = ScanSummary::default();
    let mut to_write = Vec::new();

    for file in walk.files {
        match known.remove(&file.path) {
            Some(old) if !old.missing && old.size == file.size && old.mtime == file.mtime => {}
            old => {
                if old.is_some() {
                    summary.updated += 1;
                } else {
                    summary.added += 1;
                }
                if let Some(media) = parse::parse(&file.parse_path) {
                    to_write.push((media, file));
                }
            }
        }
    }

    for title in group(to_write) {
        write_title(&tx, &title, now)?;
    }

    for (path, old) in known {
        let hidden_by_error = walk
            .unreadable
            .iter()
            .any(|dir| Path::new(&path).starts_with(dir));
        if !old.missing && !hidden_by_error {
            tx.execute("UPDATE files SET missing = 1 WHERE id = ?1", [old.id])?;
            summary.missing += 1;
        }
    }

    tx.execute(
        "DELETE FROM titles WHERE id NOT IN (SELECT title_id FROM files)",
        [],
    )?;
    tx.commit()?;
    Ok(summary)
}

fn known_files(tx: &Transaction<'_>) -> AppResult<HashMap<String, KnownFile>> {
    let mut statement = tx.prepare("SELECT path, id, size, mtime, missing FROM files")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get(0)?,
            KnownFile {
                id: row.get(1)?,
                size: row.get(2)?,
                mtime: row.get(3)?,
                missing: row.get(4)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn write_title(tx: &Transaction<'_>, title: &TitleGroup<FoundFile>, now: i64) -> AppResult<()> {
    let title_id: i64 = tx.query_row(
        "INSERT INTO titles (kind, name_key, guessed_title, guessed_year)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (kind, name_key)
         DO UPDATE SET guessed_year = COALESCE(titles.guessed_year, excluded.guessed_year)
         RETURNING id",
        params![title.kind.as_str(), title.name_key, title.title, title.year],
        |row| row.get(0),
    )?;
    for (identity, file) in &title.files {
        tx.execute(
            "INSERT INTO files (path, title_id, season, episode, size, mtime, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (path) DO UPDATE SET
               title_id = excluded.title_id, season = excluded.season,
               episode = excluded.episode, size = excluded.size,
               mtime = excluded.mtime, missing = 0",
            params![
                file.path,
                title_id,
                identity.season,
                identity.episode,
                file.size,
                file.mtime,
                now
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::db::{self, queries::list_titles};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("popcorn-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn touch(&self, relative: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"video").unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scan(conn: &mut Connection, root: &Path) -> ScanSummary {
        let walk = walk(root, |_| {}).unwrap();
        apply(conn, walk, 100).unwrap()
    }

    #[test]
    fn adds_hides_and_restores_files() {
        let dir = TempDir::new("scan");
        dir.touch("The.Matrix.1999.mkv");
        dir.touch("Dark/Season 1/Episode 01.mkv");
        dir.touch("Dark/Season 1/Episode 02.mkv");
        dir.touch(".trash/Hidden.mkv");
        dir.touch("movie-sample.mkv");
        let mut conn = db::open_in_memory().unwrap();

        assert_eq!(scan(&mut conn, &dir.0).added, 3);
        let titles = list_titles(&conn).unwrap();
        assert_eq!(titles.len(), 2);
        assert_eq!(titles[0].title, "Dark");
        assert_eq!(titles[0].file_count, 2);
        assert!(!scan(&mut conn, &dir.0).changed());

        let movie = dir.0.join("The.Matrix.1999.mkv");
        conn.execute(
            "INSERT INTO progress (file_id, position, updated_at)
             SELECT id, 90, 1 FROM files WHERE path = ?1",
            [movie.to_str().unwrap()],
        )
        .unwrap();
        fs::remove_file(&movie).unwrap();
        assert_eq!(scan(&mut conn, &dir.0).missing, 1);
        assert_eq!(list_titles(&conn).unwrap().len(), 1);

        dir.touch("The.Matrix.1999.mkv");
        assert_eq!(scan(&mut conn, &dir.0).updated, 1);
        let kept: i64 = conn
            .query_row("SELECT COUNT(*) FROM progress", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kept, 1);
    }

    #[test]
    fn an_unavailable_folder_is_an_error() {
        let result = walk(Path::new("/definitely/not/here"), |_| {});
        assert!(matches!(result, Err(AppError::FolderUnavailable)));
    }
}
