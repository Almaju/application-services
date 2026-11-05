/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::ads_store::store::AdsStoreHolder;
use crate::ads_store::AdsStore;
use crate::bytesize::ByteSize;
use crate::db::AdsDb;
use crate::telemetry;
use sql_support::open_database;
use std::path::PathBuf;

const DEFAULT_MAX_SIZE: ByteSize = ByteSize::mib(10);
const MIN_STORE_SIZE: ByteSize = ByteSize::kib(1);
const MAX_STORE_SIZE: ByteSize = ByteSize::mib(100);

#[derive(Debug, thiserror::Error)]
pub enum AdsStoreBuilderError {
    #[error("Database error: {0}")]
    Database(#[from] open_database::Error),
    #[error("Database path cannot be empty")]
    EmptyDbPath,
    #[error(
        "Maximum store size must be between {min_size} and {max_size}, got {size_bytes} bytes"
    )]
    InvalidMaxSize {
        max_size: String,
        min_size: String,
        size_bytes: u64,
    },
}

pub struct AdsStoreBuilder {
    db_path: Option<PathBuf>,
    max_size: Option<ByteSize>,
    shared_db: Option<AdsDb>,
}

impl AdsStoreBuilder {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: Some(db_path.into()),
            max_size: None,
            shared_db: None,
        }
    }

    /// A builder for a store kept in `db`, an already open db shared with the
    /// rest of the client.
    pub(crate) fn with_db(db: AdsDb) -> Self {
        Self {
            db_path: None,
            max_size: None,
            shared_db: Some(db),
        }
    }

    pub fn build(&self) -> Result<AdsStore, AdsStoreBuilderError> {
        self.validate()?;

        let db = match &self.shared_db {
            Some(db) => db.clone(),
            // Fall back to a memory db if the file db fails to open.
            None => AdsDb::open_or_memory(self.db_path.as_deref(), |e| {
                telemetry::record_ads_store_error(&AdsStoreBuilderError::from(e))
            })?,
        };
        let holder = AdsStoreHolder::new(db);
        let max_size = self.max_size.unwrap_or(DEFAULT_MAX_SIZE);
        Ok(AdsStore { holder, max_size })
    }

    pub fn max_size(mut self, max_size: ByteSize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    fn validate(&self) -> Result<(), AdsStoreBuilderError> {
        if self
            .db_path
            .as_ref()
            .is_some_and(|p| p.to_string_lossy().trim().is_empty())
        {
            return Err(AdsStoreBuilderError::EmptyDbPath);
        }

        if let Some(max_size) = self.max_size {
            if max_size < MIN_STORE_SIZE || max_size > MAX_STORE_SIZE {
                return Err(AdsStoreBuilderError::InvalidMaxSize {
                    max_size: MAX_STORE_SIZE.to_string(),
                    min_size: MIN_STORE_SIZE.to_string(),
                    size_bytes: max_size.as_u64(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IN_MEMORY_DB_PATH;

    fn make_test_builder(path: &str) -> AdsStoreBuilder {
        AdsStoreBuilder::new(path)
    }

    #[test]
    fn test_store_builder_with_defaults() {
        let builder = make_test_builder(IN_MEMORY_DB_PATH);
        assert_eq!(builder.db_path, Some(PathBuf::from(IN_MEMORY_DB_PATH)));
        assert_eq!(builder.max_size, None);
        assert!(builder.build().is_ok());
    }

    #[test]
    fn test_unopenable_path_falls_back_to_memory_db() {
        // The fallback records telemetry, so don't race the telemetry tests.
        let _lock = crate::telemetry::test_lock();
        let store = make_test_builder("this/is/a/test/bad/path")
            .build()
            .unwrap();
        assert!(store.is_memory().unwrap());
    }

    #[test]
    fn test_cache_builder_valid_custom() {
        let builder = make_test_builder(IN_MEMORY_DB_PATH).max_size(ByteSize::b(1024));

        assert_eq!(builder.db_path, Some(PathBuf::from(IN_MEMORY_DB_PATH)));
        assert_eq!(builder.max_size, Some(ByteSize::b(1024)));
        assert!(builder.build().is_ok());
    }

    #[test]
    fn test_validation_empty_db_path() {
        let result = make_test_builder("   ").build();
        assert!(matches!(result, Err(AdsStoreBuilderError::EmptyDbPath)));
    }

    #[test]
    fn test_validation_max_size_too_small() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(512))
            .build();
        assert!(matches!(
            result,
            Err(AdsStoreBuilderError::InvalidMaxSize {
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
            Err(AdsStoreBuilderError::InvalidMaxSize {
                size_bytes: 2147483648,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_boundaries() {
        let builder_min = make_test_builder(IN_MEMORY_DB_PATH).max_size(MIN_STORE_SIZE);
        assert!(builder_min.build().is_ok());

        let builder_max = make_test_builder(IN_MEMORY_DB_PATH).max_size(MAX_STORE_SIZE);
        assert!(builder_max.build().is_ok());
    }
}
