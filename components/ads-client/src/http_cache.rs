/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod builder;
mod connection_initializer;
mod outcome;
mod request_hash;
mod store;
mod strategy;
mod ttl;

use self::{
    builder::HttpCacheBuilder,
    store::HttpCacheStore,
    strategy::{CacheFirst, NetworkFirst},
};
use crate::bytesize::ByteSize;
use crate::cache_control::CacheControl;

use serde::{de::DeserializeOwned, Serialize};
use sql_support::open_database;

pub use self::builder::HttpCacheBuilderError;
pub use self::outcome::CacheOutcome;
pub use self::request_hash::RequestHash;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub enum CachePolicy {
    CacheFirst { ttl: Option<Duration> },
    NetworkFirst { ttl: Option<Duration> },
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self::CacheFirst { ttl: None }
    }
}

pub struct HttpCache {
    default_ttl: Duration,
    max_size: ByteSize,
    store: HttpCacheStore,
}

impl HttpCache {
    pub fn builder<P: AsRef<Path>>(db_path: P) -> HttpCacheBuilder {
        HttpCacheBuilder::new(db_path.as_ref())
    }

    pub fn clear(&self) -> Result<(), open_database::Error> {
        self.store.clear_all()?;
        Ok(())
    }

    pub fn invalidate_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), open_database::Error> {
        self.store.invalidate_by_hash(request_hash)?;
        Ok(())
    }

    /// Returns the value cached under `hash`, or the one `fetch` gets, as
    /// `policy` decides. A fetched value is stored for as long as its
    /// `Cache-Control` and the policy allow.
    pub fn fetch_with_policy<T, E>(
        &self,
        hash: &RequestHash,
        policy: &CachePolicy,
        fetch: impl FnOnce() -> Result<(T, CacheControl), E>,
    ) -> Result<(T, Vec<CacheOutcome>), E>
    where
        T: Serialize + DeserializeOwned,
    {
        let mut outcomes = vec![];

        // Clean up expired entries before applying the policy
        if let Err(e) = self.store.delete_expired_entries() {
            outcomes.push(CacheOutcome::CleanupFailed(e));
        }

        // Apply the cache policy and collect outcomes
        let (value, mut strategy_outcomes) = match policy {
            CachePolicy::CacheFirst { ttl } => CacheFirst {
                default_ttl: self.default_ttl,
                explicit_ttl: *ttl,
                hash: hash.clone(),
            }
            .apply(&self.store, fetch),
            CachePolicy::NetworkFirst { ttl } => NetworkFirst {
                default_ttl: self.default_ttl,
                explicit_ttl: *ttl,
                hash: hash.clone(),
            }
            .apply(&self.store, fetch),
        }?;
        outcomes.append(&mut strategy_outcomes);

        // Trim the cache to the max size only when something was actually stored
        if outcomes
            .iter()
            .any(|o| matches!(o, CacheOutcome::MissStored))
        {
            if let Err(e) = self.store.trim_to_max_size(&self.max_size) {
                outcomes.push(CacheOutcome::TrimFailed(e));
            }
        }

        Ok((value, outcomes))
    }

    pub fn shutdown_db(&self) {
        self.store.close();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::convert::Infallible;

    use serde_json::{json, Value};

    use super::*;

    /// A fetch that counts its calls and answers with `cache_control`.
    fn fetch(
        calls: &Cell<u32>,
        cache_control: CacheControl,
    ) -> impl FnOnce() -> Result<(Value, CacheControl), Infallible> + '_ {
        move || {
            calls.set(calls.get() + 1);
            Ok((json!({"ok": true}), cache_control))
        }
    }

    fn max_age(secs: u64) -> CacheControl {
        CacheControl {
            max_age: Some(secs),
            ..Default::default()
        }
    }

    fn make_hash() -> RequestHash {
        RequestHash::new(&("POST", "https://example.com/ads"))
    }

    fn make_cache() -> HttpCache {
        // Our store opens an in-memory cache for tests. So the name is irrelevant.
        HttpCache::builder("ignored_in_tests.db")
            .default_ttl(Duration::from_secs(60))
            .max_size(ByteSize::mib(1))
            .build()
            .expect("cache build should succeed")
    }

    fn make_cache_with_ttl(secs: u64) -> HttpCache {
        // In tests our store uses an in-memory DB; filename is irrelevant.
        HttpCache::builder("ignored_in_tests.db")
            .default_ttl(Duration::from_secs(secs))
            .max_size(ByteSize::mib(4))
            .build_for_time_dependent_tests()
            .expect("cache build should succeed")
    }

    #[test]
    fn test_http_cache_creation() {
        // Test that HttpCache can be created successfully with test config
        let cache: Result<HttpCache, _> = HttpCache::builder("test_cache.db").build();
        assert!(cache.is_ok());

        // Test with custom config
        let cache_with_config: Result<HttpCache, _> = HttpCache::builder("custom_test.db")
            .max_size(ByteSize::mib(1))
            .default_ttl(Duration::from_secs(60))
            .build();
        assert!(cache_with_config.is_ok());
    }

    #[test]
    fn test_clear_cache() {
        let cache: HttpCache = HttpCache::builder("test_clear.db").build().unwrap();
        let hash = RequestHash::new(&("Get", "https://example.com/test"));

        cache
            .store
            .store_with_ttl(&hash, &"test response", &Duration::from_secs(300))
            .unwrap();

        // Verify it's cached
        let retrieved = cache.store.lookup::<Value>(&hash).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        cache.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = cache.store.lookup::<Value>(&hash).unwrap();
        assert!(retrieved_after_clear.is_none());
    }

    #[test]
    fn test_default_policy_miss_then_store_then_hit() {
        let cache = make_cache();
        let calls = Cell::new(0);

        // First call: miss -> store
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Second call: hit, without fetching again
        let (value, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::Hit));
        assert_eq!(value, json!({"ok": true}));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn test_refresh_policy_always_uses_network_then_caches() {
        let cache = make_cache();
        let calls = Cell::new(0);
        let policy = CachePolicy::NetworkFirst { ttl: None };

        // First refresh: live -> MissStored
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &policy,
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Second refresh: live again, still MissStored
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &policy,
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn test_not_cacheable_no_store() {
        let cache = make_cache();
        let calls = Cell::new(0);
        let no_store = CacheControl {
            no_store: true,
            ..Default::default()
        };

        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, no_store.clone()),
            )
            .unwrap();
        assert!(matches!(
            outcomes.last().unwrap(),
            CacheOutcome::MissNotCacheable
        ));

        // Next call should fetch again (since we didn't cache)
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, no_store),
            )
            .unwrap();
        assert!(matches!(
            outcomes.last().unwrap(),
            CacheOutcome::MissNotCacheable
        ));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn test_fetch_error_is_returned_and_not_cached() {
        let cache = make_cache();
        let calls = Cell::new(0);

        let result = cache.fetch_with_policy(&make_hash(), &CachePolicy::default(), || {
            Err::<(Value, CacheControl), _>("MARS is down")
        });
        assert_eq!(result.unwrap_err(), "MARS is down");
        assert!(cache.store.lookup::<Value>(&make_hash()).unwrap().is_none());

        // The next call goes back to the network.
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn test_undecodable_entry_is_a_lookup_failure_then_refetched() {
        let cache = make_cache();
        cache
            .store
            .store_with_ttl(&make_hash(), &"not a map", &Duration::from_secs(300))
            .unwrap();

        let (value, outcomes) = cache
            .fetch_with_policy(&make_hash(), &CachePolicy::default(), || {
                Ok::<_, Infallible>((
                    HashMap::from([("ok".to_string(), true)]),
                    CacheControl::default(),
                ))
            })
            .unwrap();
        assert!(matches!(outcomes[0], CacheOutcome::LookupFailed(_)));
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        assert_eq!(value, HashMap::from([("ok".to_string(), true)]));
    }

    #[test]
    fn ttl_resolution_explicit_overrides_server_max_age() {
        let cache = make_cache_with_ttl(300);
        let calls = Cell::new(0);
        let hash = make_hash();
        let policy = CachePolicy::CacheFirst {
            ttl: Some(Duration::from_secs(20)), // Caller asked for 20s
        };

        // Caller's explicit TTL wins over the server's max-age (1s).
        let (_, outcomes) = cache
            .fetch_with_policy(&hash, &policy, fetch(&calls, max_age(1)))
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Past the server max-age (1s) but still under the explicit TTL (20s) — entry is still there.
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_some());

        // Past the explicit TTL — entry should be expired.
        cache.store.get_clock().advance(20);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_none());
    }

    #[test]
    fn ttl_resolution_uses_server_max_age_when_no_explicit_override() {
        // Configured default is 300s — should be ignored in favor of server max-age.
        let cache = make_cache_with_ttl(300);
        let calls = Cell::new(0);
        let hash = make_hash();

        let (_, outcomes) = cache
            .fetch_with_policy(&hash, &CachePolicy::default(), fetch(&calls, max_age(2)))
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Still cached at ~1s.
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_some());

        // Expired after exceeding server max-age.
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_none());
    }

    #[test]
    fn ttl_resolution_explicit_overrides_default() {
        let cache = make_cache_with_ttl(60);
        let calls = Cell::new(0);
        let hash = make_hash();
        let policy = CachePolicy::CacheFirst {
            ttl: Some(Duration::from_secs(2)),
        };

        // Store with effective TTL = 2s
        let (_, outcomes) = cache
            .fetch_with_policy(&hash, &policy, fetch(&calls, CacheControl::default()))
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Not expired yet at ~1s
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_some());

        // Expired after ~2s
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_none());
    }

    #[test]
    fn ttl_resolution_uses_default_when_no_server_and_no_request_override() {
        let cache = make_cache_with_ttl(2);
        let calls = Cell::new(0);
        let hash = make_hash();

        // Store with effective TTL = 2s from client default
        let (_, outcomes) = cache
            .fetch_with_policy(
                &hash,
                &CachePolicy::default(),
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Not expired at ~1s
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_some());

        // Expired after ~3s
        cache.store.get_clock().advance(3);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup::<Value>(&hash).unwrap().is_none());
    }

    #[test]
    fn test_expired_entry_is_a_miss_on_next_send() {
        let cache = make_cache_with_ttl(2);
        let calls = Cell::new(0);

        // First call: miss -> store with 2s TTL
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Advance clock past the TTL
        cache.store.get_clock().advance(3);

        // Second call: expired entry must be a miss, not a hit
        let (_, outcomes) = cache
            .fetch_with_policy(
                &make_hash(),
                &CachePolicy::default(),
                fetch(&calls, CacheControl::default()),
            )
            .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn test_invalidate_by_hash() {
        let cache: HttpCache = HttpCache::builder("test_invalidate.db").build().unwrap();

        let hash1 = RequestHash::new(&("Post", "https://example.com/api1"));
        let hash2 = RequestHash::new(&("Post", "https://example.com/api2"));

        cache
            .store
            .store_with_ttl(&hash1, &"test response", &Duration::from_secs(300))
            .unwrap();

        cache
            .store
            .store_with_ttl(&hash2, &"test response", &Duration::from_secs(300))
            .unwrap();

        assert!(cache.store.lookup::<Value>(&hash1).unwrap().is_some());
        assert!(cache.store.lookup::<Value>(&hash2).unwrap().is_some());

        cache.invalidate_by_hash(&hash1).unwrap();

        assert!(cache.store.lookup::<Value>(&hash1).unwrap().is_none());
        assert!(cache.store.lookup::<Value>(&hash2).unwrap().is_some());
    }
}
