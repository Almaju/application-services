/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The one SQLite database the client keeps. The HTTP cache and the ads store
//! are tables in it, and share a single connection.

mod connection_initializer;

use crate::IN_MEMORY_DB_PATH;
pub use connection_initializer::AdsDbConnectionInitializer;
use rusqlite::OpenFlags;
use sql_support::{open_database, LazyDb};
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;

/// A handle on the shared database. Clones share one connection.
///
/// Every query takes the connection's lock, so never call into the HTTP cache
/// while holding a lock taken for the ads store, or the other way around:
/// `LazyDb::lock` is not reentrant and would deadlock.
#[derive(Clone)]
pub struct AdsDb(Arc<LazyDb<AdsDbConnectionInitializer>>);

impl AdsDb {
    /// Opens the db at `path`, eagerly opening the connection so failures
    /// surface here.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, open_database::Error> {
        let db = LazyDb::new(
            path.as_ref(),
            OpenFlags::default(),
            AdsDbConnectionInitializer,
        );
        drop(db.lock()?);
        Ok(Self(Arc::new(db)))
    }

    pub fn open_memory() -> Result<Self, open_database::Error> {
        Self::open(IN_MEMORY_DB_PATH)
    }

    /// Opens the db at `path`, or a memory db if there is no path or it fails
    /// to open. `on_file_error` sees why the file failed.
    pub fn open_or_memory(
        path: Option<&Path>,
        on_file_error: impl FnOnce(open_database::Error),
    ) -> Result<Self, open_database::Error> {
        if let Some(path) = path {
            match Self::open(path) {
                Ok(db) => return Ok(db),
                Err(e) => on_file_error(e),
            }
        }
        Self::open_memory()
    }

    /// Returns whether the db is in memory rather than backed by a file.
    pub fn is_memory(&self) -> Result<bool, open_database::Error> {
        let (conn, _) = self.lock()?;
        // SQLite reports an empty path for in-memory dbs.
        Ok(conn.path() == Some(""))
    }
}

impl Deref for AdsDb {
    type Target = LazyDb<AdsDbConnectionInitializer>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clones_share_one_memory_db() {
        let db = AdsDb::open_memory().unwrap();
        let other = db.clone();
        db.lock()
            .unwrap()
            .0
            .execute(
                "INSERT INTO ads (stored_at, placement_id, ad_body, size_bytes)
                 VALUES (0, 'p', x'00', 1)",
                [],
            )
            .unwrap();
        let count: i64 = other
            .lock()
            .unwrap()
            .0
            .query_row("SELECT COUNT(*) FROM ads", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
