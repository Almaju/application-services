/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Public operations against a mock MARS, with the metrics read back from a
//! capturing sink.

use std::sync::Arc;

use mockito::mock;
use serde_json::{json, Value};
use url::Url;

use super::report;
use super::sink::{self, Capture, Rate, RequestDuration};
use crate::test_utils::get_example_happy_image_response;
use crate::{
    MozAdsCacheConfig, MozAdsClient, MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest,
};

/// MARS endpoints live under this path, so other tests' mocks never answer.
const BASE: &str = "/telemetry-tests";

struct Harness {
    capture: Arc<Capture>,
    _restore: sink::Restore,
    _lock: parking_lot::MutexGuard<'static, ()>,
}

fn harness() -> Harness {
    let lock = super::test_lock();
    viaduct_dev::init_backend_dev();
    report::reset();
    let capture = Arc::new(Capture::default());
    let restore = sink::replace(capture.clone());
    Harness {
        capture,
        _restore: restore,
        _lock: lock,
    }
}

fn client(cache_config: Option<MozAdsCacheConfig>) -> MozAdsClient {
    let base = Url::parse(&format!("{}{BASE}/", mockito::server_url())).unwrap();
    let mut builder =
        Arc::new(MozAdsClientBuilder::new()).environment(MozAdsEnvironment::Custom(base));
    if let Some(cache_config) = cache_config {
        builder = builder.cache_config(cache_config);
    }
    builder.build()
}

fn placement() -> Vec<MozAdsPlacementRequest> {
    vec![MozAdsPlacementRequest {
        iab_content: None,
        placement_id: "example_placement_1".into(),
    }]
}

fn happy_body() -> Value {
    serde_json::to_value(&get_example_happy_image_response().data).unwrap()
}

#[test]
fn test_successful_request() {
    let h = harness();
    let m = mock("POST", format!("{BASE}/ads").as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(happy_body().to_string())
        .create();

    client(None).request_image_ads(placement(), None).unwrap();
    m.assert();

    assert_eq!(h.capture.rate(Rate::RequestSuccess), (1, 1));
    // Two placements come back but only one was requested: the caller gets one.
    assert_eq!(h.capture.rate(Rate::Fill), (2, 1));
    // Cache-first by default, with no cache configured.
    assert_eq!(h.capture.rate(Rate::CacheHit), (0, 1));
    let durations = h.capture.durations.lock().clone();
    assert_eq!(durations.len(), 1);
    assert_eq!(durations[0].0, RequestDuration::Direct);
    assert!(*h.capture.context_id_rotations.lock() >= 1);
    assert!(h.capture.expected_errors.lock().is_empty());
    assert!(h.capture.error_reports.lock().is_empty());
}

#[test]
fn test_server_error_is_expected() {
    let h = harness();
    let _m = mock("POST", format!("{BASE}/ads").as_str())
        .with_status(503)
        .with_body("upstream unavailable")
        .create();

    assert!(client(None).request_image_ads(placement(), None).is_err());

    assert_eq!(h.capture.rate(Rate::RequestSuccess), (0, 1));
    assert_eq!(h.capture.rate(Rate::Fill), (0, 0));
    // Failures count towards latency too.
    assert_eq!(h.capture.durations.lock().len(), 1);
    let events = h.capture.expected_errors.lock().clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "http_server");
    assert_eq!(events[0].operation, "request_image_ads");
    assert_eq!(events[0].stage, "request");
    assert_eq!(events[0].source, Some("network"));
    assert_eq!(events[0].code.as_deref(), Some("503"));
    assert!(events[0].fatal);
    assert!(h.capture.error_reports.lock().is_empty());
}

#[test]
fn test_invalid_ad_items_are_one_unexpected_error() {
    let h = harness();
    let mut body = happy_body();
    let items = body["example_placement_1"].as_array_mut().unwrap();
    let mut broken = items[0].clone();
    broken["image_url"] = json!("not a url ?secret=1");
    items.push(broken.clone());
    items.push(broken);
    body["example_placement_3"] = json!({ "not": "an array" });
    let _m = mock("POST", format!("{BASE}/ads").as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body.to_string())
        .create();

    client(None).request_image_ads(placement(), None).unwrap();

    assert_eq!(h.capture.rate(Rate::RequestSuccess), (1, 1));
    let reports = h.capture.error_reports.lock().clone();
    assert_eq!(reports.len(), 1);
    let (payload, reason) = &reports[0];
    assert_eq!(*reason, "error");
    let records = payload["records"].as_array().unwrap();
    let item = records
        .iter()
        .find(|r| r["kind"] == "invalid_ad_item")
        .expect("invalid items are reported");
    assert_eq!(item["op"], "request_image_ads");
    assert_eq!(item["stage"], "parse");
    assert_eq!(item["fatal"], false);
    assert!(item["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s.as_str().unwrap().starts_with("parse@mars.rs:")));
    let context: Vec<(&str, &str)> = item["context"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["k"].as_str().unwrap(), e["v"].as_str().unwrap()))
        .collect();
    assert!(context.contains(&("invalid_items", "2")));
    assert!(context.contains(&("serde_path", "image_url")));
    assert!(records.iter().any(|r| r["kind"] == "invalid_placement"));
    assert!(!payload.to_string().contains("secret"));
}

#[test]
fn test_callbacks() {
    let h = harness();
    let client = client(None);
    let ok = mock("GET", format!("{BASE}/impression").as_str())
        .with_status(200)
        .create();
    let gone = mock("GET", format!("{BASE}/click").as_str())
        .with_status(404)
        .create();
    let url = |path: &str| format!("{}{BASE}/{path}", mockito::server_url());

    client.record_impression(url("impression"), None).unwrap();
    assert!(client.record_click(url("click"), None).is_err());
    assert!(client.record_click("not a url".into(), None).is_err());
    ok.assert();
    gone.assert();

    assert_eq!(h.capture.rate(Rate::ImpressionSuccess), (1, 1));
    assert_eq!(h.capture.rate(Rate::ClickSuccess), (0, 2));
    // Callbacks are not ad requests.
    assert_eq!(h.capture.rate(Rate::RequestSuccess), (0, 0));
    assert!(h.capture.durations.lock().is_empty());

    let events = h.capture.expected_errors.lock().clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "callback_gone");
    assert_eq!(events[0].stage, "callback");

    let reports = h.capture.error_reports.lock().clone();
    let record = &reports[0].0["records"][0];
    assert_eq!(record["kind"], "invalid_callback_url");
    assert_eq!(record["stage"], "start");
    assert_eq!(record["fatal"], true);
}

#[test]
fn test_build_errors_wait_for_the_first_operation() {
    let h = harness();
    let client = client(Some(MozAdsCacheConfig {
        db_path: "test_build_errors_wait_for_the_first_operation".into(),
        default_cache_ttl_seconds: None,
        max_size_mib: Some(1000),
    }));
    assert!(h.capture.error_reports.lock().is_empty());

    let _m = mock("GET", format!("{BASE}/impression").as_str())
        .with_status(200)
        .create();
    client
        .record_impression(format!("{}{BASE}/impression", mockito::server_url()), None)
        .unwrap();

    let reports = h.capture.error_reports.lock().clone();
    let record = &reports[0].0["records"][0];
    assert_eq!(record["kind"], "cache_config");
    assert_eq!(record["op"], "client_build");
    assert_eq!(record["stage"], "open");
}

#[test]
fn test_repeats_are_sent_at_shutdown() {
    let h = harness();
    let client = client(None);
    client.record_click("not a url".into(), None).unwrap_err();
    client.record_click("not a url".into(), None).unwrap_err();
    assert_eq!(h.capture.error_reports.lock().len(), 1);

    client.shutdown().unwrap();
    let reports = h.capture.error_reports.lock().clone();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[1].1, "shutdown");
    assert_eq!(reports[1].0["records"][0]["count"], 1);
}

#[test]
fn test_recorded_values_are_documented() {
    use super::classify::ErrorKind;
    use super::scope::{Operation, Stage};

    let docs = include_str!("../../docs/telemetry.md");
    let names = ErrorKind::ALL
        .iter()
        .map(|k| k.as_str())
        .chain(Operation::ALL.iter().map(|o| o.as_str()))
        .chain(Stage::ALL.iter().map(|s| s.as_str()));
    for name in names {
        assert!(
            docs.contains(&format!("| `{name}` |")),
            "`{name}` is missing from docs/telemetry.md"
        );
    }
}

#[test]
fn test_builder_installs_the_sink_until_shutdown() {
    use crate::{MozAdsExpectedError, MozAdsRate, MozAdsRequestDuration, MozAdsTelemetrySink};
    use parking_lot::Mutex;

    #[derive(Default)]
    struct Recorded {
        rates: Vec<(MozAdsRate, i32, i32)>,
        reports: Vec<String>,
    }

    struct RecordingSink(Arc<Mutex<Recorded>>);

    impl MozAdsTelemetrySink for RecordingSink {
        fn accumulate_request_duration(&self, _duration: MozAdsRequestDuration, _millis: i64) {}
        fn add_context_id_rotations(&self, _amount: i32) {}
        fn add_to_rate(&self, rate: MozAdsRate, numerator: i32, denominator: i32) {
            self.0.lock().rates.push((rate, numerator, denominator));
        }
        fn record_expected_error(&self, _error: MozAdsExpectedError) {}
        fn submit_error_reports(&self, json: String, _reason: String) {
            self.0.lock().reports.push(json);
        }
    }

    let _lock = super::test_lock();
    report::reset();
    let _restore = sink::replace_with_none();
    let recorded = Arc::new(Mutex::new(Recorded::default()));
    let client = Arc::new(MozAdsClientBuilder::new())
        .telemetry_sink(Box::new(RecordingSink(recorded.clone())))
        .build();

    client.record_click("not a url".into(), None).unwrap_err();
    assert_eq!(
        recorded.lock().rates,
        vec![(MozAdsRate::ClickSuccess, 0, 1)]
    );
    assert_eq!(recorded.lock().reports.len(), 1);

    client.shutdown().unwrap();
    assert!(sink::sink().is_none());
}
