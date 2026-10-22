/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Records into the app's Glean through glean-sym.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

use super::sink::{ExpectedErrorEvent, Rate, RequestDuration, Sink};

#[allow(clippy::all, unused)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/glean_metrics.rs"));
}

use generated::ads_client as metrics;

pub struct GleanSink;

// glean-sym panics when it cannot find the app's Glean, and keeps panicking
// on every later call. The first panic turns recording off for the process.
static DISABLED: AtomicBool = AtomicBool::new(false);

fn guarded(record: impl FnOnce()) {
    if DISABLED.load(Ordering::Relaxed) {
        return;
    }
    if catch_unwind(AssertUnwindSafe(record)).is_err() {
        DISABLED.store(true, Ordering::Relaxed);
        error_support::warn!("glean-sym panicked: ads-client stops recording metrics");
    }
}

impl Sink for GleanSink {
    fn ready(&self) -> bool {
        host_loaded()
    }

    fn accumulate_duration(&self, duration: RequestDuration, millis: i64) {
        guarded(|| match duration {
            RequestDuration::Direct => {
                metrics::request_duration_direct.accumulate_single_sample(millis)
            }
            RequestDuration::Ohttp => {
                metrics::request_duration_ohttp.accumulate_single_sample(millis)
            }
        });
    }

    fn accumulate_context_id_lifetime(&self, minutes: i64) {
        guarded(|| metrics::context_id_lifetime.accumulate_single_sample(minutes));
    }

    fn add_to_rate(&self, rate: Rate, numerator: i32, denominator: i32) {
        guarded(|| {
            let metric = match rate {
                Rate::CacheHit => &metrics::cache_hit_rate,
                Rate::ClickSuccess => &metrics::click_success_rate,
                Rate::Fill => &metrics::fill_rate,
                Rate::ImpressionSuccess => &metrics::impression_success_rate,
                Rate::RequestSuccess => &metrics::request_success_rate,
            };
            metric.add_to_numerator(numerator);
            metric.add_to_denominator(denominator);
        });
    }

    fn record_expected_error(&self, event: ExpectedErrorEvent) {
        guarded(|| {
            metrics::expected_error.record(metrics::ExpectedErrorExtra {
                code: event.code,
                fatal: Some(event.fatal),
                kind: Some(event.kind.to_string()),
                ohttp: Some(event.ohttp),
                operation: Some(event.operation.to_string()),
                stage: Some(event.stage.to_string()),
            })
        });
    }

    fn submit_error_reports(&self, json: String, reason: &'static str) {
        guarded(|| {
            metrics::error_reports.set_string(json);
            generated::ads_client_errors.submit(Some(reason));
        });
    }
}

/// glean-sym finds Glean in libxul, which GeckoView loads. Opening it before
/// then would load it outside of GeckoView, so wait until it is there.
#[cfg(target_os = "android")]
fn host_loaded() -> bool {
    // SAFETY: RTLD_NOLOAD only returns a handle when the library is already
    // loaded, and that handle is released right away.
    unsafe {
        let handle = libc::dlopen(c"libxul.so".as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY);
        if handle.is_null() {
            return false;
        }
        libc::dlclose(handle);
    }
    true
}

#[cfg(not(target_os = "android"))]
fn host_loaded() -> bool {
    true
}
