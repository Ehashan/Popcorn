//! Opening the SQLite database, plus the queries the app needs.

pub mod metadata;
mod migrations;
pub mod queries;

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;

pub fn open(path: &Path) -> AppResult<Connection> {
    prepare(Connection::open(path)?)
}

#[cfg(test)]
pub fn open_in_memory() -> AppResult<Connection> {
    prepare(Connection::open_in_memory()?)
}

fn prepare(mut conn: Connection) -> AppResult<Connection> {
    conn.pragma_update(None, "foreign_keys", true)?;
    // Returns the resulting mode as a row; in-memory databases ignore it.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    migrations::run(&mut conn)?;
    Ok(conn)
}
