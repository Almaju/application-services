/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::Connection;
use sql_support::open_database;
use std::time::Duration;

pub struct HttpCacheConnectionInitializer {}

impl open_database::ConnectionInitializer for HttpCacheConnectionInitializer {
    const END_VERSION: u32 = 3;
    const NAME: &'static str = "http_cache";

    fn init(&self, tx: &rusqlite::Transaction<'_>) -> open_database::Result<()> {
        const SCHEMA: &str = "
            CREATE TABLE IF NOT EXISTS http_cache (
                cached_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                request_hash TEXT NOT NULL,
                response_body BLOB NOT NULL,
                size_bytes INTEGER NOT NULL,
                ttl_seconds INTEGER NOT NULL,
                PRIMARY KEY (request_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_http_cache_cached_at ON http_cache(cached_at);
            CREATE INDEX IF NOT EXISTS idx_http_cache_expires_at ON http_cache(expires_at);
            CREATE INDEX IF NOT EXISTS idx_http_cache_request_hash ON http_cache(request_hash);
        ";
        // If the schema fails to initialize, it might be corrupted or outdated so we drop the table and try again
        if tx.execute_batch(SCHEMA).is_err() {
            tx.execute_batch("DROP TABLE IF EXISTS http_cache")?;
            tx.execute_batch(SCHEMA)?;
        }
        Ok(())
    }

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> open_database::Result<()> {
        conn.execute_batch("PRAGMA journal_mode=wal;")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    }

    fn upgrade_from(
        &self,
        conn: &rusqlite::Transaction<'_>,
        version: u32,
    ) -> open_database::Result<()> {
        match version {
            0 => self.init(conn),
            1 => {
                conn.execute_batch(
                    "ALTER TABLE http_cache ADD COLUMN request_method TEXT NOT NULL DEFAULT 'GET';
                     ALTER TABLE http_cache ADD COLUMN request_url TEXT NOT NULL DEFAULT 'https://unknown';",
                )?;
                Ok(())
            }
            // Entries now hold the deserialized value of a successful response,
            // stored as JSON, so the HTTP-only columns go. Old bodies are MARS
            // JSON and still decode; error responses are no longer cached.
            2 => {
                conn.execute_batch(
                    "DELETE FROM http_cache WHERE response_status != 200;
                     ALTER TABLE http_cache DROP COLUMN request_method;
                     ALTER TABLE http_cache DROP COLUMN request_url;
                     ALTER TABLE http_cache DROP COLUMN response_headers;
                     ALTER TABLE http_cache DROP COLUMN response_status;",
                )?;
                Ok(())
            }
            _ => Err(open_database::Error::IncompatibleVersion(version)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use sql_support::open_database::ConnectionInitializer;

    #[test]
    fn test_corrupted_schema_is_recreated() {
        let mut conn = Connection::open_in_memory().unwrap();
        let initializer = HttpCacheConnectionInitializer {};

        // Create a corrupted table with only one column
        conn.execute_batch("CREATE TABLE http_cache (request_hash TEXT);")
            .unwrap();

        // Run init - should drop the corrupted table and recreate it properly
        let tx = conn.transaction().unwrap();
        initializer.init(&tx).unwrap();
        tx.commit().unwrap();

        // Verify the table was recreated with correct schema by checking column count
        let column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('http_cache')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(
            column_count > 1,
            "Table should have more than 1 column after recreation"
        );
    }

    #[test]
    fn test_upgrade_from_v2_keeps_only_successful_bodies() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE http_cache (
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
            INSERT INTO http_cache VALUES
                (0, 300, 'ok', 'POST', 'https://example.com', CAST('{\"ok\":true}' AS BLOB), NULL, 200, 11, 300),
                (0, 300, 'error', 'POST', 'https://example.com', CAST('Server error' AS BLOB), NULL, 500, 12, 300);",
        )
        .unwrap();

        let tx = conn.transaction().unwrap();
        HttpCacheConnectionInitializer {}
            .upgrade_from(&tx, 2)
            .unwrap();
        tx.commit().unwrap();

        let rows: Vec<(String, Vec<u8>)> = conn
            .prepare("SELECT request_hash, response_body FROM http_cache")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![("ok".to_string(), br#"{"ok":true}"#.to_vec())]);

        let column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('http_cache')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(column_count, 6);
    }
}
