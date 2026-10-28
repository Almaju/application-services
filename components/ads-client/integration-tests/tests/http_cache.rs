/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::cell::Cell;
use std::convert::Infallible;
use std::path::Path;
use std::time::Duration;

use ads_client::bytesize::ByteSize;
use ads_client::cache_control::CacheControl;
use ads_client::http_cache::{CacheOutcome, CachePolicy, HttpCache, RequestHash};
use serde_json::{json, Value};

// Deletes existing temporary dbs passed to `clear_test_dbs` to reset tests that require making them.
fn clear_test_dbs(test_db_locations: &[&'static str]) {
    for test_db_location in test_db_locations {
        let test_db_location_path = Path::new(test_db_location);

        // A small sanity check to ensure the passed db test is a subfile of the current directory.
        if test_db_location_path.components().count() != 1 || !test_db_location_path.is_relative() {
            panic!("Tests provided to `clear_test_dbs` must single-component and in the current directory: {test_db_location}");
        }
        match std::fs::remove_file(test_db_location_path) {
            Ok(()) => println!("Deleted previous test's `{test_db_location}`"),
            Err(e) => println!("Did not delete previous test's `{test_db_location}`: {e}"),
        }
    }
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_cache_works_using_real_timeouts() {
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

    let hash = RequestHash::new(&("POST", "https://ads.mozilla.org/v1/ads"));
    let calls = Cell::new(0);
    let fetch = || {
        calls.set(calls.get() + 1);
        Ok::<_, Infallible>((json!({"ok": true}), CacheControl::default()))
    };
    let test_ttl = 2;

    // First call: miss -> store
    let (_, outcomes) = cache
        .fetch_with_policy(
            &hash,
            &CachePolicy::CacheFirst {
                ttl: Some(Duration::from_secs(test_ttl)),
            },
            fetch,
        )
        .unwrap();
    assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));

    // Second call: hit, without fetching again
    let (value, outcomes) = cache
        .fetch_with_policy(&hash, &CachePolicy::default(), fetch)
        .unwrap();
    assert!(matches!(outcomes.last().unwrap(), CacheOutcome::Hit));
    assert_eq!(value, json!({"ok": true}));
    assert_eq!(calls.get(), 1);

    // Third call: Miss due to timeout for the test_ttl duration
    std::thread::sleep(Duration::from_secs(test_ttl));
    let (_, outcomes): (Value, _) = cache
        .fetch_with_policy(&hash, &CachePolicy::default(), fetch)
        .unwrap();
    assert!(matches!(outcomes.last().unwrap(), CacheOutcome::MissStored));
    assert_eq!(calls.get(), 2);
}

#[test]
#[ignore = "integration test: run manually with -- --ignored"]
fn test_bad_path_opens_memory_db() {
    clear_test_dbs(&["good_path.db"]);

    let good_path = HttpCache::builder("good_path_httpstore.db")
        .build()
        .expect("Should be able to build a bad path");
    assert!(
        !good_path.is_memory().unwrap(),
        "A valid path should result in a file-based sqlite database"
    );

    let bad_path = HttpCache::builder("this/is/a/test/bad/path")
        .build()
        .expect("Should be able to build a bad path");
    assert!(
        bad_path.is_memory().unwrap(),
        "Invalid path should result in a memory-based sqlite database"
    );

    let empty_path = HttpCache::builder("").build();
    assert!(
        empty_path.is_err(),
        "An empty string for a path should result in an error (because of validation step)"
    );

    let no_path = HttpCache::builder_memory()
        .build()
        .expect("Should be able to build a memory db");
    assert!(
        no_path.is_memory().unwrap(),
        "`builder_memory` should result in a memory-based sqlite database"
    )
}
