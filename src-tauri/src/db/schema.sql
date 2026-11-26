CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- A movie or a TV show
CREATE TABLE titles (
  id            INTEGER PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (kind IN ('movie','show')),
  name_key      TEXT NOT NULL,
  guessed_title TEXT NOT NULL,
  guessed_year  INTEGER,
  tmdb_id       INTEGER,
  match_locked  INTEGER NOT NULL DEFAULT 0,
  metadata_json TEXT,
  poster_path   TEXT,
  backdrop_path TEXT,
  fetched_at    INTEGER,
  UNIQUE (kind, name_key)
);

-- A video file on disk (a movie file or one episode)
CREATE TABLE files (
  id        INTEGER PRIMARY KEY,
  path      TEXT NOT NULL UNIQUE,
  title_id  INTEGER NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
  season    INTEGER,
  episode   INTEGER,
  size      INTEGER NOT NULL,
  mtime     INTEGER NOT NULL,
  added_at  INTEGER NOT NULL,
  missing   INTEGER NOT NULL DEFAULT 0,
  width     INTEGER,                    -- filled in once the file has been played
  height    INTEGER
);
CREATE INDEX files_title ON files (title_id);

CREATE TABLE episode_meta (
  title_id INTEGER NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
  season   INTEGER NOT NULL,
  episode  INTEGER NOT NULL,
  name     TEXT,
  overview TEXT,
  still_path TEXT,
  PRIMARY KEY (title_id, season, episode)
);

CREATE TABLE progress (
  file_id    INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
  position   REAL NOT NULL DEFAULT 0,
  duration   REAL,
  watched    INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);
