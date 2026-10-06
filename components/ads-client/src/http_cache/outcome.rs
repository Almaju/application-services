/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::telemetry;

#[derive(Debug)]
pub enum CacheOutcome {
    CleanupFailed(rusqlite::Error), // cleaning expired objects failed
    Hit,                            // cache hit
    LookupFailed(rusqlite::Error),  // cache miss path due to lookup error
    MissNotCacheable,               // policy says "don't store"
    MissStored,                     // stored successfully
    NoCache,                        // send policy requested a cache bypass
    StoreFailed(rusqlite::Error),   // insert/upsert failed
    TrimFailed(rusqlite::Error),    // size trim failed
}

impl telemetry::HttpCacheOutcome for CacheOutcome {
    fn label(&self) -> &'static str {
        match self {
            Self::CleanupFailed(_) => "cleanup_failed",
            Self::Hit => "hit",
            Self::LookupFailed(_) => "lookup_failed",
            Self::MissNotCacheable => "miss_not_cacheable",
            Self::MissStored => "miss_stored",
            Self::NoCache => "no_cache",
            Self::StoreFailed(_) => "store_failed",
            Self::TrimFailed(_) => "trim_failed",
        }
    }

    fn value(&self) -> String {
        match self {
            Self::CleanupFailed(e)
            | Self::LookupFailed(e)
            | Self::StoreFailed(e)
            | Self::TrimFailed(e) => e.to_string(),
            Self::Hit | Self::MissNotCacheable | Self::MissStored | Self::NoCache => String::new(),
        }
    }
}
