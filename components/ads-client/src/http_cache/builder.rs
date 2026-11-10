/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::connection_initializer::HttpCacheConnectionInitializer;
use super::store::HttpCacheStore;
use crate::bytesize::ByteSize;
use crate::clock::Clock;
#[cfg(test)]
use crate::clock::MockClock;
use crate::http_cache::HttpCache;
use crate::telemetry;
use crate::IN_MEMORY_DB_PATH;
use rusqlite::OpenFlags;
use sql_support::{open_database, LazyDb};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_MAX_SIZE: ByteSize = ByteSize::mib(10);
const DEFAULT_TTL: Duration = Duration::from_secs(300);

const MIN_CACHE_SIZE: ByteSize = ByteSize::kib(1);
const MAX_CACHE_SIZE: ByteSize = ByteSize::mib(100);
const MIN_TTL: Duration = Duration::from_secs(1);
const MAX_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7); // 7 days

#[derive(Debug, thiserror::Error)]
pub enum HttpCacheBuilderError {
    /// The file db failed to open. The builder falls back to a memory db, so
    /// this is only recorded, never returned.
    #[error("Database error: {0}")]
    Database(#[from] open_database::Error),
    /// The memory db failed to open, so the build failed. Recorded apart from
    /// `Database`, as `database_memory_error`, to tell how often the fallback
    /// fails too.
    // TODO: Remove this once memory dbs confirmed to be OK.
    #[error("Database (memory) error: {0}")]
    DatabaseMemory(open_database::Error),
    #[error("Database path cannot be empty")]
    EmptyDbPath,
    #[error(
        "Maximum cache size must be between {min_size} and {max_size}, got {size_bytes} bytes"
    )]
    InvalidMaxSize {
        max_size: String,
        min_size: String,
        size_bytes: u64,
    },
    #[error("TTL must be between {min_ttl} and {max_ttl}, got {ttl} seconds")]
    InvalidTtl {
        max_ttl: String,
        min_ttl: String,
        ttl: u64,
    },
}

pub struct HttpCacheBuilder {
    clock: Clock,
    db_path: Option<PathBuf>,
    default_ttl: Option<Duration>,
    max_size: Option<ByteSize>,
    /// Where the memory db opens. Only tests change it, to make it fail.
    memory_db_path: PathBuf,
}

impl HttpCacheBuilder {
    // `db_path`: location of sqlite file for file-based db. If `None`, the db will be created in-memory.
    pub fn new(db_path: Option<impl Into<PathBuf>>) -> Self {
        Self {
            clock: Clock::default(),
            db_path: db_path.map(Into::into),
            default_ttl: None,
            max_size: None,
            memory_db_path: PathBuf::from(IN_MEMORY_DB_PATH),
        }
    }

    /// Builds the cache on a file db at `db_path`, or on a memory db if there
    /// is no path or the file db fails to open. If the memory db fails to open
    /// too, the build fails.
    ///
    /// Every error is recorded against `ads_client.build_cache_error`: the
    /// file db failing as `database_error`, even though the build goes on in
    /// memory, and the memory db failing as `database_memory_error`.
    pub fn build(&self) -> Result<HttpCache, HttpCacheBuilderError> {
        self.try_build()
            .inspect_err(telemetry::record_build_cache_error)
    }

    fn try_build(&self) -> Result<HttpCache, HttpCacheBuilderError> {
        self.validate()?;

        Ok(HttpCache {
            clock: self.clock.clone(),
            default_ttl: self.default_ttl.unwrap_or(DEFAULT_TTL),
            max_size: self.max_size.unwrap_or(DEFAULT_MAX_SIZE),
            store: HttpCacheStore::new(self.open_db()?),
        })
    }

    /// Replaces the system clock with `clock`, for tests that move time.
    #[cfg(test)]
    pub(crate) fn mock_clock(mut self, clock: MockClock) -> Self {
        self.clock = clock.into();
        self
    }

    /// Opens the memory db at `path` instead, for tests that make it fail.
    #[cfg(test)]
    pub(crate) fn memory_db_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.memory_db_path = path.into();
        self
    }

    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    pub fn max_size(mut self, max_size: ByteSize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    // Opens the db at `db_path`, or a memory db if there is no path or it fails to open.
    fn open_db(&self) -> Result<LazyDb<HttpCacheConnectionInitializer>, HttpCacheBuilderError> {
        if let Some(db_path) = &self.db_path {
            match open_connection(db_path) {
                Ok(db) => return Ok(db),
                // Not returned, since we fall back to memory, so record it here.
                Err(e) => telemetry::record_build_cache_error(&HttpCacheBuilderError::Database(e)),
            }
        }

        open_connection(&self.memory_db_path).map_err(HttpCacheBuilderError::DatabaseMemory)
    }

    fn validate(&self) -> Result<(), HttpCacheBuilderError> {
        if self
            .db_path
            .as_ref()
            .is_some_and(|p| p.to_string_lossy().trim().is_empty())
        {
            return Err(HttpCacheBuilderError::EmptyDbPath);
        }

        if let Some(max_size) = self.max_size {
            if max_size < MIN_CACHE_SIZE || max_size > MAX_CACHE_SIZE {
                return Err(HttpCacheBuilderError::InvalidMaxSize {
                    max_size: MAX_CACHE_SIZE.to_string(),
                    min_size: MIN_CACHE_SIZE.to_string(),
                    size_bytes: max_size.as_u64(),
                });
            }
        }

        if let Some(ttl) = self.default_ttl {
            if !(MIN_TTL..=MAX_TTL).contains(&ttl) {
                return Err(HttpCacheBuilderError::InvalidTtl {
                    max_ttl: format!("{} seconds", MAX_TTL.as_secs()),
                    min_ttl: format!("{} seconds", MIN_TTL.as_secs()),
                    ttl: ttl.as_secs(),
                });
            }
        }

        Ok(())
    }
}

// Opens the db at `path`, eagerly opening the initial connection so failures surface here.
fn open_connection(
    path: &Path,
) -> Result<LazyDb<HttpCacheConnectionInitializer>, open_database::Error> {
    let db = LazyDb::new(
        path,
        OpenFlags::default(),
        HttpCacheConnectionInitializer {},
    );
    drop(db.lock()?);
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::RecordingTelemetry;

    /// A path no db can open, since its directories do not exist.
    const UNOPENABLE_PATH: &str = "this/is/a/test/bad/path";

    fn make_test_builder(path: &str) -> HttpCacheBuilder {
        HttpCacheBuilder::new(Some(path))
    }

    /// Builds with `builder`, returning what it recorded against
    /// `ads_client.build_cache_error`, by label.
    fn build_recording(
        builder: &HttpCacheBuilder,
    ) -> (
        Result<HttpCache, HttpCacheBuilderError>,
        std::collections::HashMap<String, Vec<String>>,
    ) {
        let _lock = telemetry::test_lock();
        let recording = RecordingTelemetry::default();
        telemetry::install(recording.clone());
        let result = builder.build();
        telemetry::uninstall();
        let records = recording.build_cache_errors.lock().clone();
        (result, records)
    }

    #[test]
    fn test_cache_builder_with_defaults() {
        let builder = make_test_builder(IN_MEMORY_DB_PATH);
        assert_eq!(builder.db_path, Some(PathBuf::from(IN_MEMORY_DB_PATH)));
        assert_eq!(builder.max_size, None);
        assert_eq!(builder.default_ttl, None);
        assert!(builder.build().is_ok());
    }

    #[test]
    fn test_openable_path_builds_file_db() {
        let path = std::env::temp_dir().join(format!(
            "ads_client_http_cache_builder_test_{}.db",
            std::process::id()
        ));
        let (cache, records) = build_recording(&HttpCacheBuilder::new(Some(&path)));
        let cache = cache.unwrap();
        assert!(!cache.is_memory().unwrap());
        // Only tests holding the telemetry lock fail to open a db, so no other
        // test records these.
        assert!(!records.contains_key("database_error"));
        assert!(!records.contains_key("database_memory_error"));

        drop(cache);
        for suffix in ["", "-shm", "-wal"] {
            let mut file = path.clone().into_os_string();
            file.push(suffix);
            // The db may not have created every file.
            let _ = std::fs::remove_file(file);
        }
    }

    #[test]
    fn test_unopenable_path_falls_back_to_memory_db() {
        let (cache, records) = build_recording(&make_test_builder(UNOPENABLE_PATH));
        assert!(cache.unwrap().is_memory().unwrap());
        // The file db failure is recorded, even though the build went on in memory.
        assert!(records["database_error"]
            .iter()
            .any(|value| value.starts_with("Database error: ")));
        assert!(!records.contains_key("database_memory_error"));
    }

    #[test]
    fn test_memory_db_failure_after_fallback_fails_the_build() {
        let (result, records) =
            build_recording(&make_test_builder(UNOPENABLE_PATH).memory_db_path(UNOPENABLE_PATH));
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::DatabaseMemory(_))
        ));
        // Both failures are recorded, each under its own label.
        assert!(records["database_error"]
            .iter()
            .any(|value| value.starts_with("Database error: ")));
        assert!(records["database_memory_error"]
            .iter()
            .any(|value| value.starts_with("Database (memory) error: ")));
    }

    #[test]
    fn test_memory_db_failure_without_path_fails_the_build() {
        let (result, records) = build_recording(
            &HttpCacheBuilder::new(None::<PathBuf>).memory_db_path(UNOPENABLE_PATH),
        );
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::DatabaseMemory(_))
        ));
        // There was no file db to fail.
        assert!(!records.contains_key("database_error"));
        assert!(records["database_memory_error"]
            .iter()
            .any(|value| value.starts_with("Database (memory) error: ")));
    }

    #[test]
    fn test_validation_error_is_recorded() {
        let (result, records) = build_recording(&make_test_builder("   "));
        assert!(matches!(result, Err(HttpCacheBuilderError::EmptyDbPath)));
        assert!(records.contains_key("empty_db_path"));
    }

    #[test]
    fn test_cache_builder_valid_custom() {
        let builder = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(1024))
            .default_ttl(Duration::from_secs(60));

        assert_eq!(builder.db_path, Some(PathBuf::from(IN_MEMORY_DB_PATH)));
        assert_eq!(builder.max_size, Some(ByteSize::b(1024)));
        assert_eq!(builder.default_ttl, Some(Duration::from_secs(60)));
        assert!(builder.build().is_ok());
    }

    #[test]
    fn test_validation_empty_db_path() {
        let result = make_test_builder("   ").build();
        assert!(matches!(result, Err(HttpCacheBuilderError::EmptyDbPath)));
    }

    #[test]
    fn test_validation_max_size_too_small() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(512))
            .build();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidMaxSize {
                size_bytes: 512,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_too_large() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(2 * 1024 * 1024 * 1024))
            .build();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidMaxSize {
                size_bytes: 2147483648,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_boundaries() {
        let builder_min = make_test_builder(IN_MEMORY_DB_PATH).max_size(MIN_CACHE_SIZE);
        assert!(builder_min.build().is_ok());

        let builder_max = make_test_builder(IN_MEMORY_DB_PATH).max_size(MAX_CACHE_SIZE);
        assert!(builder_max.build().is_ok());
    }

    #[test]
    fn test_validation_ttl_too_small() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .default_ttl(Duration::from_secs(0))
            .build();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidTtl {
                ttl: 0,
                min_ttl: _,
                max_ttl: _,
            })
        ));
    }

    #[test]
    fn test_validation_ttl_too_large() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .default_ttl(Duration::from_secs(8 * 24 * 60 * 60))
            .build();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidTtl {
                ttl: 691200,
                min_ttl: _,
                max_ttl: _,
            })
        ));
    }

    #[test]
    fn test_validation_ttl_boundaries() {
        let builder_min = make_test_builder(IN_MEMORY_DB_PATH).default_ttl(MIN_TTL);
        assert!(builder_min.build().is_ok());

        let builder_max = make_test_builder(IN_MEMORY_DB_PATH).default_ttl(MAX_TTL);
        assert!(builder_max.build().is_ok());
    }
}
