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
//! Errors are split in two while the new telemetry replaces the old:
//!
//! - The `record_*` functions record the metrics in `metrics.yaml` through
//!   the [`Telemetry`] callback the consumer hands to
//!   `MozAdsClientBuilder::telemetry()`; see `telemetry/backend.rs`. They now
//!   only get expected errors, such as timeouts and 5xx responses.
//! - `begin`, `step!` and `report_side_error` send unexpected errors, such as
//!   responses that do not deserialize or SQL errors, as a list of records in
//!   the `ads-client-errors` ping, through the sink the consumer hands to
//!   `MozAdsClientBuilder::telemetry_sink()`; see `report.rs` and
//!   `sink.rs`. `classify.rs` decides which errors are unexpected, and
//!   `docs/telemetry.md` describes the records.
//!
//! Every function here is infallible and silent by design. Telemetry must never
//! change the outcome of the operation it is describing.
//!
//! [`glean-sym`]: https://github.com/mozilla/glean/tree/main/glean-core/glean-sym

use std::fmt::Display;

use crate::http_cache::{CacheOutcome, HttpCacheBuilderError};
use classify::{Classify, Tier};

mod backend;
mod classify;
mod report;
mod scope;
mod scrub;
pub(crate) mod sink;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use backend::test_lock;
pub use backend::Telemetry;
pub(crate) use backend::{install, uninstall};
pub(crate) use classify::{InvalidAdItems, InvalidPlacement};
pub(crate) use scope::{begin, note_cache_outcome, report_side_error, shutdown, Operation, Stage};

/// Marks the step the current operation is at, for the operation's trace and
/// to tell where an error happened.
macro_rules! step {
    ($stage:expr) => {
        $crate::telemetry::step_at($stage, concat!(file!(), ":", line!()))
    };
}
#[doc(hidden)]
pub(crate) use scope::step as step_at;
pub(crate) use step;

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

/// A label of `ads_client.build_cache_error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildCacheErrorLabel {
    DatabaseError,
    EmptyDbPath,
    InvalidMaxSize,
    InvalidTtl,
    // TODO: these are not in `metrics.yaml`, so Glean records them as
    // `__other__`. Give the ads store its own metric.
    #[cfg(feature = "stateful")]
    StoreDatabaseError,
    #[cfg(feature = "stateful")]
    StoreEmptyDbPath,
    #[cfg(feature = "stateful")]
    StoreInvalidMaxSize,
}

impl BuildCacheErrorLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::DatabaseError => "database_error",
            Self::EmptyDbPath => "empty_db_path",
            Self::InvalidMaxSize => "invalid_max_size",
            Self::InvalidTtl => "invalid_ttl",
            #[cfg(feature = "stateful")]
            Self::StoreDatabaseError => "store_database_error",
            #[cfg(feature = "stateful")]
            Self::StoreEmptyDbPath => "store_empty_db_path",
            #[cfg(feature = "stateful")]
            Self::StoreInvalidMaxSize => "store_invalid_max_size",
        }
    }
}

/// A label of `ads_client.http_cache_outcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpCacheOutcomeLabel {
    CleanupFailed,
    Hit,
    LookupFailed,
    MissNotCacheable,
    MissStored,
    NoCache,
    StoreFailed,
    TrimFailed,
}

impl HttpCacheOutcomeLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::CleanupFailed => "cleanup_failed",
            Self::Hit => "hit",
            Self::LookupFailed => "lookup_failed",
            Self::MissNotCacheable => "miss_not_cacheable",
            Self::MissStored => "miss_stored",
            Self::NoCache => "no_cache",
            Self::StoreFailed => "store_failed",
            Self::TrimFailed => "trim_failed",
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
/// Unexpected errors are left to the `ads-client-errors` ping.
pub fn record_client_error(operation: ClientOperation, error: &(impl Display + Classify)) {
    if !is_expected(error) {
        return;
    }
    backend::client_error(operation.label(), error.to_string());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failure to build the HTTP cache against
/// `ads_client.build_cache_error`.
pub fn record_build_cache_error(error: &HttpCacheBuilderError) {
    if !is_expected(error) {
        return;
    }
    let label = match error {
        HttpCacheBuilderError::Database(_) => BuildCacheErrorLabel::DatabaseError,
        HttpCacheBuilderError::EmptyDbPath => BuildCacheErrorLabel::EmptyDbPath,
        HttpCacheBuilderError::InvalidMaxSize { .. } => BuildCacheErrorLabel::InvalidMaxSize,
        HttpCacheBuilderError::InvalidTtl { .. } => BuildCacheErrorLabel::InvalidTtl,
    };
    backend::build_cache_error(label.as_str(), error.to_string());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failure to build the ads store against
/// `ads_client.build_cache_error`.
#[cfg(feature = "stateful")]
pub fn record_ads_store_error(error: &crate::ads_store::builder::AdsStoreBuilderError) {
    // By design: telemetry owns every label, even though the ads store
    // records through it.
    use crate::ads_store::builder::AdsStoreBuilderError;

    if !is_expected(error) {
        return;
    }
    let label = match error {
        AdsStoreBuilderError::Database(_) => BuildCacheErrorLabel::StoreDatabaseError,
        AdsStoreBuilderError::EmptyDbPath => BuildCacheErrorLabel::StoreEmptyDbPath,
        AdsStoreBuilderError::InvalidMaxSize { .. } => BuildCacheErrorLabel::StoreInvalidMaxSize,
    };
    backend::build_cache_error(label.as_str(), error.to_string());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records the result of an HTTP cache read against
/// `ads_client.http_cache_outcome`.
pub fn record_http_cache_outcome(outcome: &CacheOutcome) {
    let failure = match outcome {
        CacheOutcome::CleanupFailed(e)
        | CacheOutcome::LookupFailed(e)
        | CacheOutcome::StoreFailed(e)
        | CacheOutcome::TrimFailed(e) => Some(e),
        CacheOutcome::Hit
        | CacheOutcome::MissNotCacheable
        | CacheOutcome::MissStored
        | CacheOutcome::NoCache => None,
    };
    if failure.is_some_and(|e| !is_expected(e)) {
        return;
    }
    let (label, value) = match outcome {
        CacheOutcome::CleanupFailed(e) => (HttpCacheOutcomeLabel::CleanupFailed, e.to_string()),
        CacheOutcome::Hit => (HttpCacheOutcomeLabel::Hit, String::new()),
        CacheOutcome::LookupFailed(e) => (HttpCacheOutcomeLabel::LookupFailed, e.to_string()),
        CacheOutcome::MissNotCacheable => (HttpCacheOutcomeLabel::MissNotCacheable, String::new()),
        CacheOutcome::MissStored => (HttpCacheOutcomeLabel::MissStored, String::new()),
        CacheOutcome::NoCache => (HttpCacheOutcomeLabel::NoCache, String::new()),
        CacheOutcome::StoreFailed(e) => (HttpCacheOutcomeLabel::StoreFailed, e.to_string()),
        CacheOutcome::TrimFailed(e) => (HttpCacheOutcomeLabel::TrimFailed, e.to_string()),
    };
    backend::http_cache_outcome(label.as_str(), value);
}

/// Reports an ad item we could not deserialize, one item at a time. Prefer
/// reporting [`InvalidAdItems`] once per response.
#[cfg(feature = "stateful")]
#[track_caller]
pub fn record_invalid_ad_item(error: &serde_json::Error) {
    report_side_error(
        Stage::Parse,
        &InvalidAdItems {
            count: 1,
            first: error,
            first_index: None,
            first_path: None,
        },
    );
}

/// Whether `error` still goes to the `MozAdsTelemetry` metrics.
fn is_expected(error: &impl Classify) -> bool {
    error.classify().tier() == Tier::Expected
}
