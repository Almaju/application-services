/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::request_hash::RequestHash;
use super::store::HttpCacheStore;
use super::ttl::EffectiveTtl;
use super::CacheOutcome;
use crate::cache_control::CacheControl;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

pub struct CacheFirst {
    pub default_ttl: Duration,
    pub explicit_ttl: Option<Duration>,
    pub hash: RequestHash,
}

impl CacheFirst {
    pub fn apply<T, E>(
        self,
        store: &HttpCacheStore,
        fetch: impl FnOnce() -> Result<(T, CacheControl), E>,
    ) -> Result<(T, Vec<CacheOutcome>), E>
    where
        T: Serialize + DeserializeOwned,
    {
        let mut outcomes = vec![];
        match store.lookup(&self.hash) {
            Ok(Some(value)) => return Ok((value, vec![CacheOutcome::Hit])),
            Err(e) => outcomes.push(CacheOutcome::LookupFailed(e)),
            Ok(None) => {}
        }

        let network = NetworkFirst {
            default_ttl: self.default_ttl,
            explicit_ttl: self.explicit_ttl,
            hash: self.hash,
        };
        let (value, mut network_outcomes) = network.apply(store, fetch)?;
        outcomes.append(&mut network_outcomes);
        Ok((value, outcomes))
    }
}

pub struct NetworkFirst {
    pub default_ttl: Duration,
    pub explicit_ttl: Option<Duration>,
    pub hash: RequestHash,
}

impl NetworkFirst {
    pub fn apply<T, E>(
        self,
        store: &HttpCacheStore,
        fetch: impl FnOnce() -> Result<(T, CacheControl), E>,
    ) -> Result<(T, Vec<CacheOutcome>), E>
    where
        T: Serialize,
    {
        let (value, cache_control) = fetch()?;
        let outcome = if cache_control.should_cache() {
            let ttl = EffectiveTtl {
                default: self.default_ttl,
                explicit: self.explicit_ttl,
                server_max_age: cache_control.max_age_duration(),
            }
            .resolve();
            if ttl.is_zero() {
                return Ok((value, vec![CacheOutcome::NoCache]));
            }
            match store.store_with_ttl(&self.hash, &value, &ttl) {
                Ok(()) => CacheOutcome::MissStored,
                Err(e) => CacheOutcome::StoreFailed(e),
            }
        } else {
            CacheOutcome::MissNotCacheable
        };
        Ok((value, vec![outcome]))
    }
}
