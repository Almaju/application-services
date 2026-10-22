/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Where the metrics recorded by `scope.rs` and `report.rs` go:
//!
//! - the `MozAdsTelemetrySink` callback the consumer hands to
//!   `MozAdsClientBuilder::telemetry_sink()`, if any. Desktop does this, since
//!   glean-sym does not run there yet;
//! - otherwise Glean through glean-sym, when the `glean-sym` feature is on, as
//!   on Android and iOS;
//! - otherwise nowhere.

use std::sync::Arc;

use parking_lot::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rate {
    CacheHit,
    ClickSuccess,
    Fill,
    ImpressionSuccess,
    RequestSuccess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestDuration {
    Direct,
    Ohttp,
}

/// One `ads_client.expected_error` event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedErrorEvent {
    pub code: Option<String>,
    pub fatal: bool,
    pub kind: &'static str,
    pub ohttp: bool,
    pub operation: &'static str,
    pub stage: &'static str,
}

pub trait Sink: Send + Sync {
    /// Whether the app's Glean can be called yet.
    fn ready(&self) -> bool {
        true
    }
    fn accumulate_duration(&self, duration: RequestDuration, millis: i64);
    /// Accumulates how long a context ID was used, in minutes.
    fn accumulate_context_id_lifetime(&self, minutes: i64);
    fn add_to_rate(&self, rate: Rate, numerator: i32, denominator: i32);
    fn record_expected_error(&self, event: ExpectedErrorEvent);
    /// Sets `ads_client.error_reports` and submits the `ads-client-errors` ping.
    fn submit_error_reports(&self, json: String, reason: &'static str);
}

// By design: process-wide, like the `Telemetry` callback and Glean.
static SINK: RwLock<Option<Arc<dyn Sink>>> = RwLock::new(None);

/// Makes `sink` the destination of every metric recorded from now on, in
/// place of glean-sym.
pub fn install(sink: impl Sink + 'static) {
    // Dropping a UniFFI callback calls into foreign code, so let the replaced
    // one drop after the lock is released.
    let _replaced = SINK.write().replace(Arc::new(sink));
}

/// Drops the installed sink, after which metrics go to glean-sym, if built
/// in, or nowhere.
pub fn uninstall() {
    let _removed = SINK.write().take();
}

pub fn sink() -> Option<Arc<dyn Sink>> {
    installed().or_else(glean_sym)
}

pub fn installed() -> Option<Arc<dyn Sink>> {
    SINK.read().clone()
}

#[cfg(feature = "glean-sym")]
fn glean_sym() -> Option<Arc<dyn Sink>> {
    static GLEAN: std::sync::LazyLock<Arc<dyn Sink>> =
        std::sync::LazyLock::new(|| Arc::new(super::glean::GleanSink));
    Some(GLEAN.clone())
}

#[cfg(not(feature = "glean-sym"))]
fn glean_sym() -> Option<Arc<dyn Sink>> {
    None
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
    pub durations: parking_lot::Mutex<Vec<(RequestDuration, i64)>>,
    pub context_id_lifetimes: parking_lot::Mutex<Vec<i64>>,
    pub rates: parking_lot::Mutex<Vec<(Rate, i32, i32)>>,
    pub expected_errors: parking_lot::Mutex<Vec<ExpectedErrorEvent>>,
    pub error_reports: parking_lot::Mutex<Vec<(serde_json::Value, &'static str)>>,
}

#[cfg(test)]
impl Capture {
    /// Numerator and denominator summed over every sample of `rate`.
    pub fn rate(&self, rate: Rate) -> (i32, i32) {
        self.rates
            .lock()
            .iter()
            .filter(|(r, ..)| *r == rate)
            .fold((0, 0), |(n, d), (_, rn, rd)| (n + rn, d + rd))
    }
}

#[cfg(test)]
impl Sink for Capture {
    fn accumulate_duration(&self, duration: RequestDuration, millis: i64) {
        self.durations.lock().push((duration, millis));
    }
    fn accumulate_context_id_lifetime(&self, minutes: i64) {
        self.context_id_lifetimes.lock().push(minutes);
    }
    fn add_to_rate(&self, rate: Rate, numerator: i32, denominator: i32) {
        self.rates.lock().push((rate, numerator, denominator));
    }
    fn record_expected_error(&self, event: ExpectedErrorEvent) {
        self.expected_errors.lock().push(event);
    }
    fn submit_error_reports(&self, json: String, reason: &'static str) {
        let value = serde_json::from_str(&json).expect("error reports are JSON");
        self.error_reports.lock().push((value, reason));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_installed_sink_comes_before_glean_sym() {
        let _lock = crate::telemetry::test_lock();
        let capture: Arc<dyn Sink> = Arc::new(Capture::default());
        {
            let _restore = replace(capture.clone());
            assert!(Arc::ptr_eq(&sink().unwrap(), &capture));
        }
        let _restore = replace_with_none();
        assert_eq!(sink().is_some(), cfg!(feature = "glean-sym"));
    }
}
