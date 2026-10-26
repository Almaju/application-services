/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

// By design: this module defines and adapts the deprecated `MozAdsTelemetry`,
// including the code UniFFI generates for it.
#![allow(deprecated)]

use std::sync::Arc;

use crate::telemetry::{self, Telemetry};

/// Receives the ads client's first metrics, for the app to record in Glean.
///
/// Deprecated: implement [`MozAdsTelemetrySink`] instead. Both receive their
/// metrics until this one is removed.
#[deprecated(note = "Implement MozAdsTelemetrySink and pass it to telemetry_sink() instead")]
#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetry: Send + Sync {
    fn record_build_cache_error(&self, label: String, value: String);
    fn record_client_error(&self, label: String, value: String);
    fn record_client_operation_total(&self, label: String);
    fn record_deserialization_error(&self, label: String, value: String);
    fn record_http_cache_outcome(&self, label: String, value: String);
}

/// Makes the consumer's callback the destination of every metric recorded from now on.
pub fn install(callback: Arc<dyn MozAdsTelemetry>) {
    telemetry::install(MozAdsTelemetryWrapper(callback));
}

/// Adapts the consumer's [`MozAdsTelemetry`] callback to the core [`Telemetry`] trait.
struct MozAdsTelemetryWrapper(Arc<dyn MozAdsTelemetry>);

impl Telemetry for MozAdsTelemetryWrapper {
    fn record_build_cache_error(&self, label: &str, value: String) {
        self.0.record_build_cache_error(label.to_string(), value);
    }

    fn record_client_error(&self, label: &str, value: String) {
        self.0.record_client_error(label.to_string(), value);
    }

    fn record_client_operation_total(&self, label: &str) {
        self.0.record_client_operation_total(label.to_string());
    }

    fn record_deserialization_error(&self, label: &str, value: String) {
        self.0
            .record_deserialization_error(label.to_string(), value);
    }

    fn record_http_cache_outcome(&self, label: &str, value: String) {
        self.0.record_http_cache_outcome(label.to_string(), value);
    }
}

/// Receives the ads client's metrics, for the app to record in Glean: one
/// method per metric in `metrics.yaml`, named after it.
#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetrySink: Send + Sync {
    fn record_request_success_rate(&self, numerator: i32, denominator: i32);
    fn record_impression_success_rate(&self, numerator: i32, denominator: i32);
    fn record_click_success_rate(&self, numerator: i32, denominator: i32);
    fn record_fill_rate(&self, numerator: i32, denominator: i32);
    fn record_cache_hit_rate(&self, numerator: i32, denominator: i32);
    fn record_request_duration_direct(&self, millis: i64);
    fn record_request_duration_ohttp(&self, millis: i64);
    fn record_context_id_lifetime(&self, minutes: i64);
    fn record_expected_error(&self, operation: Option<String>, kind: String, code: Option<String>);
    fn record_unexpected_error(
        &self,
        operation: Option<String>,
        kind: String,
        code: Option<String>,
        detail: Option<String>,
    );
}
