/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod builder;
mod cache_control;
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
use crate::mars::ad_response::AdResponse;

use sql_support::open_database;

pub use self::builder::HttpCacheBuilderError;
pub(crate) use self::cache_control::CacheControl;
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
        HttpCacheBuilder::new(Some(db_path.as_ref()))
    }

    pub fn builder_memory() -> HttpCacheBuilder {
        HttpCacheBuilder::new(None::<String>)
    }

    pub fn is_memory(&self) -> Result<bool, open_database::Error> {
        self.store.is_memory()
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

    /// Returns the ads cached under `hash`, or the ones `fetch` gets, as
    /// `policy` decides. Fetched ads are stored for as long as their
    /// `Cache-Control` and the policy allow. `fetch` returns an error for a
    /// response that must not be cached, such as an error status.
    pub(crate) fn fetch_with_policy<E>(
        &self,
        hash: &RequestHash,
        policy: &CachePolicy,
        fetch: impl FnOnce() -> Result<(AdResponse, CacheControl), E>,
    ) -> Result<(AdResponse, Vec<CacheOutcome>), E> {
        let mut outcomes = vec![];

        // Clean up expired entries before applying the policy
        if let Err(e) = self.store.delete_expired_entries() {
            outcomes.push(CacheOutcome::CleanupFailed(e));
        }

        // Apply the cache policy and collect outcomes
        let (response, mut strategy_outcomes) = match policy {
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
            if let Err(e) = self.store.trim_to_max_size(self.max_size) {
                outcomes.push(CacheOutcome::TrimFailed(e));
            }
        }

        Ok((response, outcomes))
    }

    pub fn shutdown_db(&self) {
        self.store.close();
    }
}

#[cfg(test)]
mod tests {
    use mockito::mock;
    use std::hash::{Hash, Hasher};

    use super::*;
    use crate::mars::error::{HTTPError, TransportError};
    use std::collections::HashMap;
    use viaduct::{Client, ClientSettings, Request};

    fn make_client() -> Client {
        Client::new(ClientSettings::default())
    }

    /// Gets `req` through the cache, sending it to the mock server and reading
    /// the ads on a miss, as `MARSClient` does.
    fn send(
        cache: &HttpCache,
        client: &Client,
        req: TestRequest,
        policy: &CachePolicy,
    ) -> Result<(AdResponse, Vec<CacheOutcome>), TransportError> {
        let hash = RequestHash::new(&req);
        cache.fetch_with_policy(&hash, policy, || {
            let response = client.send_sync(req.into())?;
            HTTPError::check(&response)?;
            Ok((response.json()?, CacheControl::from(&response)))
        })
    }

    /// Test-only hashable wrapper around Request.
    /// Hashes method + url for cache key purposes.
    #[derive(Clone)]
    struct TestRequest(Request);

    impl Hash for TestRequest {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.0.method.as_str().hash(state);
            self.0.url.as_str().hash(state);
        }
    }

    impl From<TestRequest> for Request {
        fn from(t: TestRequest) -> Self {
            t.0
        }
    }

    fn make_post_request() -> TestRequest {
        let url = format!("{}/ads", mockito::server_url()).parse().unwrap();
        TestRequest(Request::post(url).json(&serde_json::json!({"fake":"data"})))
    }

    fn make_cache() -> HttpCache {
        // Our store opens an in-memory cache for tests. So the name is irrelevant.
        HttpCache::builder_memory()
            .default_ttl(Duration::from_secs(60))
            .max_size(ByteSize::mib(1))
            .build()
            .expect("cache build should succeed")
    }

    fn make_cache_with_ttl(secs: u64) -> HttpCache {
        // In tests our store uses an in-memory DB; filename is irrelevant.
        HttpCache::builder_memory()
            .default_ttl(Duration::from_secs(secs))
            .max_size(ByteSize::mib(4))
            .build_for_time_dependent_tests()
            .expect("cache build should succeed")
    }

    #[test]
    fn test_http_cache_creation() {
        // Test that HttpCache can be created successfully with test config
        let cache: Result<HttpCache, _> = HttpCache::builder_memory().build();
        assert!(cache.is_ok());

        // Test with custom config
        let cache_with_config: Result<HttpCache, _> = HttpCache::builder_memory()
            .max_size(ByteSize::mib(1))
            .default_ttl(Duration::from_secs(60))
            .build();
        assert!(cache_with_config.is_ok());
    }

    #[test]
    fn test_clear_cache() {
        let cache: HttpCache = HttpCache::builder_memory().build().unwrap();

        let hash = RequestHash::new(&("Get", "https://example.com/test"));

        let response = AdResponse {
            data: HashMap::from([("test response".to_owned(), vec![])]),
        };

        cache
            .store
            .store_with_ttl(&hash, &response, &Duration::from_secs(300))
            .unwrap();

        // Verify it's cached
        let retrieved = cache.store.lookup(&hash).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        cache.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = cache.store.lookup(&hash).unwrap();
        assert!(retrieved_after_clear.is_none());
    }

    #[test]
    fn test_default_policy_miss_then_store_then_hit() {
        viaduct_dev::init_backend_dev();

        let body = r#"{"ok":true}"#;
        let m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(1) // only the first call should hit the network
            .create();

        let cache = make_cache();
        let req = make_post_request();
        let client = make_client();

        // First call: miss -> store
        let (_, outcomes) = send(&cache, &client, req.clone(), &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Second call: hit (no extra HTTP request due to expect(1))
        let (response, outcomes) = send(&cache, &client, req, &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::Hit));
        assert!(response.data.is_empty());
        m.assert();
    }

    #[test]
    fn test_refresh_policy_always_uses_network_then_caches() {
        viaduct_dev::init_backend_dev();

        let body = r#"{"ok":true}"#;
        let m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(2)
            .create();

        let cache = make_cache();
        let req = make_post_request();
        let client = make_client();

        // First refresh: live -> MissStored
        let (_, outcomes) = send(
            &cache,
            &client,
            req.clone(),
            &CachePolicy::NetworkFirst { ttl: None },
        )
        .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Second refresh: live again, still MissStored
        let (response, outcomes) = send(
            &cache,
            &client,
            req,
            &CachePolicy::NetworkFirst { ttl: None },
        )
        .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        assert!(response.data.is_empty());
        m.assert();
    }

    #[test]
    fn test_not_cacheable_no_store() {
        viaduct_dev::init_backend_dev();

        let m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "no-store") // should block caching
            .with_body(r#"{"ok":true}"#)
            .expect(2) // both calls should hit the network since the response isn't cacheable
            .create();

        let cache = make_cache();
        let req = make_post_request();
        let client = make_client();

        let (_, outcomes) = send(&cache, &client, req.clone(), &CachePolicy::default()).unwrap();
        assert!(matches!(
            outcomes.last().unwrap(),
            CacheOutcome::MissNotCacheable
        ));

        // Next call should hit network again (since we didn't cache)
        let (_, outcomes) = send(&cache, &client, req, &CachePolicy::default()).unwrap();
        assert!(matches!(
            outcomes.last().unwrap(),
            CacheOutcome::MissNotCacheable
        ));
        m.assert();
    }

    #[test]
    fn test_error_response_is_not_cached() {
        viaduct_dev::init_backend_dev();

        let m = mock("POST", "/ads")
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error":"unavailable"}"#)
            .expect(2) // both calls should hit the network: an error is not cached
            .create();

        let cache = make_cache();
        let req = make_post_request();
        let client = make_client();

        for _ in 0..2 {
            let result = send(&cache, &client, req.clone(), &CachePolicy::default());
            assert!(matches!(result, Err(TransportError::Http(_))));
        }
        m.assert();
    }

    #[test]
    fn ttl_resolution_explicit_overrides_server_max_age() {
        viaduct_dev::init_backend_dev();

        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "max-age=1") // Server says 1 second
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache_with_ttl(300);
        let req = make_post_request();
        let hash = RequestHash::new(&req);
        let policy = CachePolicy::CacheFirst {
            ttl: Some(Duration::from_secs(20)), // Caller asked for 20s
        };

        let client = make_client();
        // Caller's explicit TTL wins over the server's max-age.
        let (_, outcomes) = send(&cache, &client, req, &policy).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Past the server max-age (1s) but still under the explicit TTL (20s) — entry is still there.
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_some());

        // Past the explicit TTL — entry should be expired.
        cache.store.get_clock().advance(20);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_none());
        m.assert();
    }

    #[test]
    fn ttl_resolution_uses_server_max_age_when_no_explicit_override() {
        viaduct_dev::init_backend_dev();

        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "max-age=2") // Server says 2 seconds
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        // Configured default is 300s — should be ignored in favor of server max-age.
        let cache = make_cache_with_ttl(300);
        let req = make_post_request();
        let hash = RequestHash::new(&req);

        let client = make_client();
        let (_, outcomes) = send(&cache, &client, req, &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Still cached at ~1s.
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_some());

        // Expired after exceeding server max-age.
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_none());
        m.assert();
    }

    #[test]
    fn ttl_resolution_explicit_overrides_default() {
        viaduct_dev::init_backend_dev();

        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache_with_ttl(60);
        let req = make_post_request();
        let hash = RequestHash::new(&req);
        let policy = CachePolicy::CacheFirst {
            ttl: Some(Duration::from_secs(2)),
        };

        let client = make_client();
        // Store with effective TTL = 2s
        let (_, outcomes) = send(&cache, &client, req, &policy).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Not expired yet at ~1s
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_some());

        // Expired after ~2s
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_none());
        m.assert();
    }

    #[test]
    fn ttl_resolution_uses_default_when_no_server_and_no_request_override() {
        viaduct_dev::init_backend_dev();

        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            // No response policy ttl
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache_with_ttl(2);
        let req = make_post_request();
        let hash = RequestHash::new(&req);
        let client = make_client();
        // Store with effective TTL = 2s from client default
        let (_, outcomes) = send(&cache, &client, req, &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Not expired at ~1s
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_some());

        // Expired after ~3s
        cache.store.get_clock().advance(3);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_none());
        m.assert();
    }

    #[test]
    fn test_expired_entry_is_a_miss_on_next_send() {
        viaduct_dev::init_backend_dev();

        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(2) // both calls should hit the network — second after the entry expires
            .create();

        let cache = make_cache_with_ttl(2);
        let req = make_post_request();
        let client = make_client();

        // First call: miss -> store with 2s TTL
        let (_, outcomes) = send(&cache, &client, req.clone(), &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Advance clock past the TTL
        cache.store.get_clock().advance(3);

        // Second call: expired entry must be a miss, not a hit
        let (_, outcomes) = send(&cache, &client, req, &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        m.assert();
    }

    #[test]
    fn test_invalidate_by_hash() {
        let cache: HttpCache = HttpCache::builder_memory().build().unwrap();

        let hash1 = RequestHash::new(&("Post", "https://example.com/api1"));
        let hash2 = RequestHash::new(&("Post", "https://example.com/api2"));

        let response = AdResponse {
            data: HashMap::from([("test response".to_owned(), vec![])]),
        };

        cache
            .store
            .store_with_ttl(&hash1, &response, &Duration::from_secs(300))
            .unwrap();

        cache
            .store
            .store_with_ttl(&hash2, &response, &Duration::from_secs(300))
            .unwrap();

        assert!(cache.store.lookup(&hash1).unwrap().is_some());
        assert!(cache.store.lookup(&hash2).unwrap().is_some());

        cache.invalidate_by_hash(&hash1).unwrap();

        assert!(cache.store.lookup(&hash1).unwrap().is_none());
        assert!(cache.store.lookup(&hash2).unwrap().is_some());
    }

    #[test]
    #[ignore = "uses a file and real timeouts: run manually with -- --ignored"]
    fn test_cache_works_using_real_timeouts() {
        viaduct_dev::init_backend_dev();

        let test_db_location = "integration_tests.db";
        match std::fs::remove_file(std::path::Path::new(test_db_location)) {
            Ok(()) => println!("Deleted previous test's `{test_db_location}`"),
            Err(e) => println!("Did not delete previous test's `{test_db_location}`: {e}"),
        }
        let cache = HttpCache::builder("integration_tests.db")
            .default_ttl(Duration::from_secs(60))
            .max_size(ByteSize::mib(1))
            .build()
            .expect("cache build should succeed");

        let url = format!("{}/v1/ads", mockito::server_url()).parse().unwrap();
        let req = TestRequest(Request::post(url).json(&serde_json::json!({
            "context_id": "12347fff-00b0-aaaa-0978-189231239808",
            "placements": [
                {
                "placement": "mock_pocket_billboard_1",
                "count": 1,
                }
            ],
        })));

        let client = Client::new(ClientSettings::default());
        let test_ttl = 2;

        let m1 = mock("POST", "/v1/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        // First call: miss -> store
        let (_, outcomes) = send(
            &cache,
            &client,
            req.clone(),
            &CachePolicy::CacheFirst {
                ttl: Some(Duration::from_secs(test_ttl)),
            },
        )
        .unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

        // Second call: hit (no extra HTTP due to expect(1))
        let (response, outcomes) =
            send(&cache, &client, req.clone(), &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::Hit));
        assert!(response.data.is_empty());
        m1.assert();

        let m2 = mock("POST", "/v1/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        // Third call: Miss due to timeout for the test_ttl duration
        std::thread::sleep(Duration::from_secs(test_ttl));
        let (response, outcomes) = send(&cache, &client, req, &CachePolicy::default()).unwrap();
        assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
        assert!(response.data.is_empty());
        m2.assert();
    }
}
