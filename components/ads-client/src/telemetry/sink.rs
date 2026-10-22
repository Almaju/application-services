/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Where the error reports from `report.rs` go: the [`Sink`]
//! the FFI layer installs, which wraps the `MozAdsTelemetrySink` callback the
//! consumer hands to `MozAdsClientBuilder::telemetry_sink()`.
//!
//! Until a sink is installed, and after it is uninstalled, recording is a
//! no-op.

use std::sync::Arc;

use parking_lot::RwLock;

pub trait Sink: Send + Sync {
    /// Sets `ads_client.error_reports` and submits the `ads-client-errors` ping.
    fn submit_error_reports(&self, json: String, reason: &'static str);
}

// By design: process-wide, like the `Telemetry` callback.
static SINK: RwLock<Option<Arc<dyn Sink>>> = RwLock::new(None);

/// Makes `sink` the destination of every metric recorded from now on.
pub fn install(sink: impl Sink + 'static) {
    // Dropping a UniFFI callback calls into foreign code, so let the replaced
    // one drop after the lock is released.
    let _replaced = SINK.write().replace(Arc::new(sink));
}

/// Drops the installed sink, after which recording is a no-op.
pub fn uninstall() {
    let _removed = SINK.write().take();
}

pub fn sink() -> Option<Arc<dyn Sink>> {
    SINK.read().clone()
}

/// Puts the previous sink back when dropped.
#[cfg(test)]
pub struct Restore(Option<Arc<dyn Sink>>);

#[cfg(test)]
impl Drop for Restore {
    fn drop(&mut self) {
        let _replaced = std::mem::replace(&mut *SINK.write(), self.0.take());
    }
}

/// Replaces the sink until the returned guard drops.
#[cfg(test)]
pub fn replace(sink: Arc<dyn Sink>) -> Restore {
    Restore(SINK.write().replace(sink))
}

/// Removes the sink until the returned guard drops.
#[cfg(test)]
pub fn replace_with_none() -> Restore {
    Restore(SINK.write().take())
}

/// A sink that keeps everything, for tests.
#[cfg(test)]
#[derive(Default)]
pub struct Capture {
    pub error_reports: parking_lot::Mutex<Vec<(serde_json::Value, &'static str)>>,
}

#[cfg(test)]
impl Sink for Capture {
    fn submit_error_reports(&self, json: String, reason: &'static str) {
        let value = serde_json::from_str(&json).expect("error reports are JSON");
        self.error_reports.lock().push((value, reason));
    }
}
