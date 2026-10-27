/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Records the metrics through the [`MozAdsTelemetrySink`] the consumer hands
//! to `MozAdsClientBuilder::telemetry_sink()`. Until one is installed, and
//! after it is uninstalled, recording is a no-op.

use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;

use super::errors::{self, ErrorKind};
use crate::client::error::ComponentError;
use crate::http_cache::CacheOutcome;
use crate::MozAdsTelemetrySink;

// By design: process-wide, like the `Telemetry` callback.
static SINK: RwLock<Option<Arc<dyn MozAdsTelemetrySink>>> = RwLock::new(None);

pub fn install(sink: Arc<dyn MozAdsTelemetrySink>) {
    // Dropping a UniFFI callback calls into foreign code, so let the replaced
    // one drop after the lock is released.
    let _replaced = SINK.write().replace(sink);
}

pub fn uninstall() {
    let _removed = SINK.write().take();
}

fn with_sink(f: impl FnOnce(&dyn MozAdsTelemetrySink)) {
    let Some(sink) = SINK.read().clone() else {
        return;
    };
    f(sink.as_ref());
}

/// Records an ad request: whether it succeeded, how long the caller waited,
/// and its error, if any.
pub fn record_ad_request<T>(
    operation: &'static str,
    ohttp: bool,
    started: Instant,
    result: &Result<T, ComponentError>,
) {
    with_sink(|sink| {
        sink.record_request_success_rate(i32::from(result.is_ok()), 1);
        let millis = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
        if ohttp {
            sink.record_request_duration_ohttp(millis);
        } else {
            sink.record_request_duration_direct(millis);
        }
    });
    if let Err(error) = result {
        record_error(Some(operation), errors::component(error));
    }
}

#[derive(Clone, Copy)]
pub enum Callback {
    Click,
    Impression,
    Report,
}

/// Records a click, impression or report callback and its error, if any.
pub fn record_callback(callback: Callback, result: &Result<(), ComponentError>) {
    let ok = i32::from(result.is_ok());
    with_sink(|sink| match callback {
        Callback::Click => sink.record_click_success_rate(ok, 1),
        Callback::Impression => sink.record_impression_success_rate(ok, 1),
        Callback::Report => {}
    });
    if let Err(error) = result {
        let operation = match callback {
            Callback::Click => "record_click",
            Callback::Impression => "record_impression",
            Callback::Report => "report_ad",
        };
        record_error(Some(operation), errors::component(error));
    }
}

/// Records an error as an `expected_error` or `unexpected_error` event.
/// `operation` is the public method it failed, if any.
pub fn record_error(operation: Option<&'static str>, error: ErrorKind) {
    let operation = operation.map(str::to_string);
    let kind = error.kind.to_string();
    with_sink(|sink| {
        if error.expected {
            sink.record_expected_error(operation, kind, error.code);
        } else {
            sink.record_unexpected_error(operation, kind, error.code, error.detail);
        }
    });
}

/// Records a failed HTTP cache read or write, which does not fail the request.
pub fn record_cache_failure(outcome: &CacheOutcome) {
    match outcome {
        CacheOutcome::CleanupFailed(e)
        | CacheOutcome::LookupFailed(e)
        | CacheOutcome::StoreFailed(e)
        | CacheOutcome::TrimFailed(e) => record_error(None, errors::database(e)),
        CacheOutcome::Hit
        | CacheOutcome::MissNotCacheable
        | CacheOutcome::MissStored
        | CacheOutcome::NoCache => {}
    }
}

/// Records whether a cache-first ad request was answered from the cache.
pub fn record_cache_lookup(hit: bool) {
    with_sink(|sink| sink.record_cache_hit_rate(i32::from(hit), 1));
}

/// Records how long a context ID was used, in seconds.
pub fn record_context_id_lifetime(seconds: i64) {
    with_sink(|sink| sink.record_context_id_lifetime(seconds.max(0) / 60));
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;
    use url::Url;

    use crate::telemetry::test_lock;
    use crate::test_utils::get_example_happy_image_response;
    use crate::{
        MozAdsClient, MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest,
        MozAdsTelemetry,
    };

    use super::*;

    /// MARS endpoints live under this path, so other tests' mocks never answer.
    const BASE: &str = "/telemetry-sink-tests";

    /// Keeps every call to either callback, as a string.
    #[derive(Clone, Default)]
    struct Calls(Arc<Mutex<Vec<String>>>);

    impl Calls {
        fn push(&self, call: String) {
            self.0.lock().push(call);
        }

        /// Checks that `expected` were recorded, in that order, among calls
        /// that other tests' clients may have added, then forgets them all.
        fn assert_recorded(&self, expected: &[&str]) {
            let calls = std::mem::take(&mut *self.0.lock());
            let mut rest = calls.iter();
            for call in expected {
                assert!(
                    rest.any(|c| c == call),
                    "{call:?} not recorded in order in {calls:?}"
                );
            }
        }
    }

    impl MozAdsTelemetry for Calls {
        fn record_build_cache_error(&self, _label: String, _value: String) {}
        fn record_client_error(&self, label: String, _value: String) {
            self.push(format!("old client_error {label}"));
        }
        fn record_client_operation_total(&self, _label: String) {}
        fn record_deserialization_error(&self, label: String, _value: String) {
            self.push(format!("old deserialization_error {label}"));
        }
        fn record_http_cache_outcome(&self, _label: String, _value: String) {}
    }

    impl MozAdsTelemetrySink for Calls {
        fn record_request_success_rate(&self, numerator: i32, denominator: i32) {
            self.push(format!("request_success_rate {numerator}/{denominator}"));
        }
        fn record_impression_success_rate(&self, numerator: i32, denominator: i32) {
            self.push(format!("impression_success_rate {numerator}/{denominator}"));
        }
        fn record_click_success_rate(&self, numerator: i32, denominator: i32) {
            self.push(format!("click_success_rate {numerator}/{denominator}"));
        }
        fn record_cache_hit_rate(&self, numerator: i32, denominator: i32) {
            self.push(format!("cache_hit_rate {numerator}/{denominator}"));
        }
        fn record_request_duration_direct(&self, _millis: i64) {
            self.push("request_duration_direct".into());
        }
        fn record_request_duration_ohttp(&self, _millis: i64) {
            self.push("request_duration_ohttp".into());
        }
        fn record_context_id_lifetime(&self, minutes: i64) {
            self.push(format!("context_id_lifetime {minutes}"));
        }
        fn record_expected_error(
            &self,
            operation: Option<String>,
            kind: String,
            code: Option<String>,
        ) {
            self.push(format!("expected_error {operation:?} {kind} {code:?}"));
        }
        fn record_unexpected_error(
            &self,
            operation: Option<String>,
            kind: String,
            code: Option<String>,
            detail: Option<String>,
        ) {
            self.push(format!(
                "unexpected_error {operation:?} {kind} {code:?} {detail:?}"
            ));
        }
    }

    fn client(calls: &Calls) -> MozAdsClient {
        viaduct_dev::init_backend_dev();
        let base = Url::parse(&format!("{}{BASE}/", mockito::server_url())).unwrap();
        Arc::new(MozAdsClientBuilder::new())
            .environment(MozAdsEnvironment::Custom(base))
            .telemetry(Box::new(calls.clone()))
            .telemetry_sink(Box::new(calls.clone()))
            .build()
    }

    fn placement() -> Vec<MozAdsPlacementRequest> {
        vec![MozAdsPlacementRequest {
            iab_content: None,
            placement_id: "example_placement_1".into(),
        }]
    }

    fn mock_ads(status: usize, body: String) -> mockito::Mock {
        mockito::mock("POST", format!("{BASE}/ads").as_str())
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create()
    }

    #[test]
    fn test_successful_request() {
        let _lock = test_lock();
        let calls = Calls::default();
        let client = client(&calls);
        let body = serde_json::to_string(&get_example_happy_image_response().data).unwrap();
        let _m = mock_ads(200, body);

        client.request_image_ads(placement(), None).unwrap();
        calls.assert_recorded(&[
            "cache_hit_rate 0/1",
            "request_success_rate 1/1",
            "request_duration_direct",
        ]);
        client.shutdown().unwrap();
    }

    #[test]
    fn test_errors_go_to_both_callbacks() {
        let _lock = test_lock();
        let calls = Calls::default();
        let client = client(&calls);

        let _m = mock_ads(503, "unavailable".into());
        client.request_image_ads(placement(), None).unwrap_err();
        calls.assert_recorded(&[
            "old client_error request_ads",
            "request_success_rate 0/1",
            "request_duration_direct",
            "expected_error Some(\"request_image_ads\") http_server Some(\"503\")",
        ]);

        client.record_click("not a url".into(), None).unwrap_err();
        calls.assert_recorded(&[
            "click_success_rate 0/1",
            "unexpected_error Some(\"record_click\") invalid_callback_url None None",
        ]);
        client.shutdown().unwrap();
    }

    #[test]
    fn test_invalid_ad_item() {
        let _lock = test_lock();
        let calls = Calls::default();
        let client = client(&calls);
        let mut body = serde_json::to_value(&get_example_happy_image_response().data).unwrap();
        let items = body["example_placement_1"].as_array_mut().unwrap();
        let mut broken = items[0].clone();
        broken["image_url"] = serde_json::json!("not a url");
        items.push(broken);
        let _m = mock_ads(200, body.to_string());

        client.request_image_ads(placement(), None).unwrap();
        calls.assert_recorded(&[
            "old deserialization_error invalid_ad_item",
            "unexpected_error None invalid_ad_item None Some(\"image_url\")",
        ]);
        client.shutdown().unwrap();
    }

    #[test]
    fn test_shutdown_records_the_context_id_lifetime_and_drops_the_sink() {
        let _lock = test_lock();
        let calls = Calls::default();
        let client = client(&calls);
        let _m = mock_ads(503, "unavailable".into());
        // Issues the context ID.
        client.request_image_ads(placement(), None).unwrap_err();
        calls.assert_recorded(&[]);

        client.shutdown().unwrap();
        calls.assert_recorded(&["context_id_lifetime 0"]);
        assert!(SINK.read().is_none());
    }
}
