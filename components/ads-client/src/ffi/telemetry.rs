/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use crate::telemetry::sink::{ExpectedErrorEvent, Rate, RequestDuration, Sink};
use crate::telemetry::{self, Telemetry};

/// Receives the ads client's metrics, for the app to record in Glean.
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

/// A success rate, as recorded in `ads_client.<name>_rate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MozAdsRate {
    CacheHit,
    ClickSuccess,
    Fill,
    ImpressionSuccess,
    RequestSuccess,
}

/// The latency distribution a request belongs to: `ads_client.request_duration_<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MozAdsRequestDuration {
    Direct,
    Ohttp,
}

/// The extras of one `ads_client.expected_error` event.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MozAdsExpectedError {
    pub code: Option<String>,
    pub fatal: bool,
    pub kind: String,
    pub ohttp: bool,
    pub operation: String,
    pub stage: String,
}

/// Receives the success metrics, expected errors and unexpected errors
/// described in `docs/telemetry.md`, for the app to record in Glean.
#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetrySink: Send + Sync {
    /// Accumulates one sample, in milliseconds.
    fn accumulate_request_duration(&self, duration: MozAdsRequestDuration, millis: i64);
    /// Accumulates how long a context ID was used, in minutes.
    fn accumulate_context_id_lifetime(&self, minutes: i64);
    fn add_to_rate(&self, rate: MozAdsRate, numerator: i32, denominator: i32);
    fn record_expected_error(&self, error: MozAdsExpectedError);
    /// Sets `ads_client.error_reports` to `json` and submits the
    /// `ads-client-errors` ping with `reason`.
    fn submit_error_reports(&self, json: String, reason: String);
}

/// Makes the consumer's sink the destination of the metrics recorded from now on.
pub fn install_sink(sink: Arc<dyn MozAdsTelemetrySink>) {
    telemetry::sink::install(MozAdsTelemetrySinkWrapper(sink));
}

/// Adapts the consumer's [`MozAdsTelemetrySink`] to the core [`Sink`] trait.
struct MozAdsTelemetrySinkWrapper(Arc<dyn MozAdsTelemetrySink>);

impl Sink for MozAdsTelemetrySinkWrapper {
    fn accumulate_duration(&self, duration: RequestDuration, millis: i64) {
        let duration = match duration {
            RequestDuration::Direct => MozAdsRequestDuration::Direct,
            RequestDuration::Ohttp => MozAdsRequestDuration::Ohttp,
        };
        self.0.accumulate_request_duration(duration, millis);
    }

    fn accumulate_context_id_lifetime(&self, minutes: i64) {
        self.0.accumulate_context_id_lifetime(minutes);
    }

    fn add_to_rate(&self, rate: Rate, numerator: i32, denominator: i32) {
        let rate = match rate {
            Rate::CacheHit => MozAdsRate::CacheHit,
            Rate::ClickSuccess => MozAdsRate::ClickSuccess,
            Rate::Fill => MozAdsRate::Fill,
            Rate::ImpressionSuccess => MozAdsRate::ImpressionSuccess,
            Rate::RequestSuccess => MozAdsRate::RequestSuccess,
        };
        self.0.add_to_rate(rate, numerator, denominator);
    }

    fn record_expected_error(&self, event: ExpectedErrorEvent) {
        self.0.record_expected_error(MozAdsExpectedError {
            code: event.code,
            fatal: event.fatal,
            kind: event.kind.to_string(),
            ohttp: event.ohttp,
            operation: event.operation.to_string(),
            stage: event.stage.to_string(),
        });
    }

    fn submit_error_reports(&self, json: String, reason: &'static str) {
        self.0.submit_error_reports(json, reason.to_string());
    }
}
