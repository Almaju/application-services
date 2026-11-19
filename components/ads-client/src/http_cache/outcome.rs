/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/
use sql_support::open_database;

use crate::telemetry::ErrorEvent;

#[derive(Debug)]
pub enum CacheOutcome {
    CleanupFailed(open_database::Error), // cleaning expired objects failed
    Hit,                                 // cache hit
    LookupFailed(open_database::Error),  // cache miss path due to lookup error
    MissNotCacheable,                    // policy says "don't store"
    MissStored,                          // stored successfully
    NoCache,                             // send policy requested a cache bypass
    StoreFailed(open_database::Error),   // insert/upsert failed
    TrimFailed(open_database::Error),    // size trim failed
}

impl CacheOutcome {
    /// Records what went wrong, if this outcome is a failure.
    pub(crate) fn record_failure(&self) {
        match self {
            Self::CleanupFailed(e)
            | Self::LookupFailed(e)
            | Self::StoreFailed(e)
            | Self::TrimFailed(e) => ErrorEvent::from(e).record(None),
            Self::Hit | Self::MissNotCacheable | Self::MissStored | Self::NoCache => {}
        }
    }
}
