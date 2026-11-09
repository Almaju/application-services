/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync + 'static {
    #[cfg(test)]
    fn advance(&self, secs: i64);
    fn now(&self) -> DateTime<Utc>;
}

pub struct CacheClock;

impl Clock for CacheClock {
    #[cfg(test)]
    fn advance(&self, _secs: i64) {
        panic!(
            "
        You cannot advance a non-test clock.
        Be sure to build the cache or store with the test clock for time-dependent tests.
    "
        )
    }
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
pub struct TestClock {
    now: std::sync::atomic::AtomicI64,
}

#[cfg(test)]
impl TestClock {
    pub fn new(start: i64) -> Self {
        Self {
            now: std::sync::atomic::AtomicI64::new(start),
        }
    }
}

#[cfg(test)]
impl Clock for TestClock {
    fn advance(&self, secs: i64) {
        self.now
            .fetch_add(secs, std::sync::atomic::Ordering::Relaxed);
    }
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.now.load(std::sync::atomic::Ordering::Relaxed), 0)
            .expect("the test clock stays within chrono's range")
    }
}
