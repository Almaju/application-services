/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{Connection, Transaction};
use sql_support::open_database;
use std::time::Duration;

/// How long a connection waits for another connection's lock before failing
/// with `SQLITE_BUSY`.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const HTTP_CACHE_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS http_cache (
        cached_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        request_hash TEXT NOT NULL,
        request_method TEXT NOT NULL,
        request_url TEXT NOT NULL,
        response_body BLOB NOT NULL,
        response_headers BLOB,
        response_status INTEGER NOT NULL,
        size_bytes INTEGER NOT NULL,
        ttl_seconds INTEGER NOT NULL,
        PRIMARY KEY (request_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_http_cache_cached_at ON http_cache(cached_at);
    CREATE INDEX IF NOT EXISTS idx_http_cache_expires_at ON http_cache(expires_at);
    CREATE INDEX IF NOT EXISTS idx_http_cache_request_hash ON http_cache(request_hash);
";

const ADS_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS ads (
        stored_at INTEGER NOT NULL,
        placement_id TEXT NOT NULL,
        ad_body BLOB NOT NULL,
        size_bytes INTEGER NOT NULL,
        PRIMARY KEY (placement_id)
    );
    CREATE INDEX IF NOT EXISTS idx_ads_stored_at ON ads(stored_at);
    CREATE INDEX IF NOT EXISTS idx_ads_placement_id ON ads(placement_id);
";

/// The schema of the whole db. The `ads` table is created even without the
/// `stateful` feature: `user_version` covers the whole file, so the schema
/// can't depend on the build.
///
/// Versions 1 and 2 are the HTTP-cache-only db that shipped before the ads
/// store, so existing cache files are upgraded in place.
pub struct AdsDbConnectionInitializer;

impl open_database::ConnectionInitializer for AdsDbConnectionInitializer {
    const END_VERSION: u32 = 3;
    const NAME: &'static str = "ads_client";

    fn init(&self, tx: &Transaction<'_>) -> open_database::Result<()> {
        create_table(tx, "http_cache", HTTP_CACHE_SCHEMA)?;
        create_table(tx, "ads", ADS_SCHEMA)
    }

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> open_database::Result<()> {
        conn.execute_batch("PRAGMA journal_mode=wal;")?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        Ok(())
    }

    fn upgrade_from(&self, tx: &Transaction<'_>, version: u32) -> open_database::Result<()> {
        match version {
            0 => self.init(tx),
            1 => {
                tx.execute_batch(
                    "ALTER TABLE http_cache ADD COLUMN request_method TEXT NOT NULL DEFAULT 'GET';
                     ALTER TABLE http_cache ADD COLUMN request_url TEXT NOT NULL DEFAULT 'https://unknown';",
                )?;
                Ok(())
            }
            2 => create_table(tx, "ads", ADS_SCHEMA),
            _ => Err(open_database::Error::IncompatibleVersion(version)),
        }
    }
}

// If the schema fails to initialize, the table might be corrupted or outdated,
// so it is dropped and created again.
fn create_table(tx: &Transaction<'_>, table: &str, schema: &str) -> open_database::Result<()> {
    if tx.execute_batch(schema).is_err() {
        tx.execute_batch(&format!("DROP TABLE IF EXISTS {table}"))?;
        tx.execute_batch(schema)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use sql_support::open_database::{open_database, ConnectionInitializer};

    fn column_count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table}')"),
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn test_corrupted_schema_is_recreated() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Create corrupted tables with only one column
        conn.execute_batch(
            "CREATE TABLE http_cache (request_hash TEXT);
             CREATE TABLE ads (placement_id TEXT);",
        )
        .unwrap();

        // Run init - should drop the corrupted tables and recreate them properly
        let tx = conn.transaction().unwrap();
        AdsDbConnectionInitializer.init(&tx).unwrap();
        tx.commit().unwrap();

        assert!(column_count(&conn, "http_cache") > 1);
        assert!(column_count(&conn, "ads") > 1);
    }

    #[test]
    fn test_http_cache_only_db_is_upgraded_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ads_cache.sqlite");

        // A cache db as shipped before the ads store: version 2, no `ads` table.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(HTTP_CACHE_SCHEMA).unwrap();
            conn.execute_batch(
                "INSERT INTO http_cache VALUES
                    (0, 9999999999, 'hash', 'GET', 'https://a', x'00', NULL, 200, 1, 300);
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        }

        let conn = open_database(&path, &AdsDbConnectionInitializer).unwrap();
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, AdsDbConnectionInitializer::END_VERSION);
        assert!(column_count(&conn, "ads") > 1);
        let cached: i64 = conn
            .query_row("SELECT COUNT(*) FROM http_cache", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cached, 1, "the upgrade keeps cached responses");
    }
}
