/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Metric recording for the ads client.
//!
//! Telemetry is a process-wide singleton: recording a metric is a plain
//! function call from wherever the interesting thing happened, much like a
//! `tracing` macro. Nothing has to be threaded through the clients, stores or
//! transports.
//!
//! This is the same API the crate will expose once metrics are recorded
//! straight from Rust through [`glean-sym`]. Until then, the backend forwards
//! to the [`MozAdsTelemetry`] callback the consumer hands to
//! `MozAdsClientBuilder::telemetry()`; see `telemetry/backend.rs`. Moving to
//! glean-sym means swapping that backend for one that talks to the generated
//! metrics, and none of the recording sites have to change.
//!
//! Every function here is infallible and silent by design. Telemetry must never
//! change the outcome of the operation it is describing.
//!
//! This module depends on nothing else in the crate: the types it records
//! implement its traits where they are defined.
//!
//! [`glean-sym`]: https://github.com/mozilla/glean/tree/main/glean-core/glean-sym

use std::fmt::Display;

mod backend;

pub use backend::MozAdsTelemetry;
pub(crate) use backend::{install, uninstall};
#[cfg(test)]
pub(crate) use backend::{test_lock, NoopMozAdsTelemetry};

/// A failure to build one of the client's databases, as labeled by
/// `ads_client.build_cache_error`. The value recorded is its `Display`.
pub trait BuildCacheError: Display {
    fn label(&self) -> &'static str;
}

/// The result of an HTTP cache read, as labeled by
/// `ads_client.http_cache_outcome`.
pub trait HttpCacheOutcome {
    fn label(&self) -> &'static str;
    /// The error behind the outcome, or an empty string if there was none.
    fn value(&self) -> String;
}

/// A client operation, as labeled by `ads_client.client_operation_total` and
/// `ads_client.client_error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientOperation {
    New,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestAds,
}

impl ClientOperation {
    fn label(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::RecordClick => "record_click",
            Self::RecordImpression => "record_impression",
            Self::ReportAd => "report_ad",
            Self::RequestAds => "request_ads",
        }
    }
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records an attempted operation against `ads_client.client_operation_total`.
pub fn record_client_operation(operation: ClientOperation) {
    backend::client_operation_total(operation.label());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failed operation against `ads_client.client_error`.
///
/// Errors are recorded here even when they are also propagated to the consumer.
pub fn record_client_error(operation: ClientOperation, error: &impl Display) {
    backend::client_error(operation.label(), error.to_string());
}

/// Records a failure to build the HTTP cache or the ads store against
/// `ads_client.build_cache_error`.
pub fn record_build_cache_error(error: &impl BuildCacheError) {
    backend::build_cache_error(error.label(), error.to_string());
}

/// Records the result of an HTTP cache read against
/// `ads_client.http_cache_outcome`.
pub fn record_http_cache_outcome(outcome: &impl HttpCacheOutcome) {
    backend::http_cache_outcome(outcome.label(), outcome.value());
}

/// Records an ad item we could not deserialize against
/// `ads_client.deserialization_error`.
pub fn record_invalid_ad_item(error: &serde_json::Error) {
    backend::deserialization_error("invalid_ad_item", error.to_string());
}
