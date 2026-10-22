/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Public operations against a mock MARS: unexpected errors are read back
//! from a capturing sink, expected ones from the `MozAdsTelemetry` metrics.

use std::sync::Arc;

use mockito::mock;
use parking_lot::Mutex;
use serde_json::{json, Value};
use url::Url;

use super::report;
use super::sink::{self, Capture};
use super::Telemetry;
use crate::test_utils::get_example_happy_image_response;
use crate::{
    MozAdsCacheConfig, MozAdsClient, MozAdsClientBuilder, MozAdsEnvironment, MozAdsPlacementRequest,
};

/// MARS endpoints live under this path, so other tests' mocks never answer.
const BASE: &str = "/telemetry-tests";

/// Keeps the `MozAdsTelemetry` metrics, as `metric:label` strings.
#[derive(Clone, Default)]
struct V1(Arc<Mutex<Vec<String>>>);

impl V1 {
    fn recorded(&self, metric: &str) -> Vec<String> {
        let prefix = format!("{metric}:");
        self.0
            .lock()
            .iter()
            .filter_map(|m| m.strip_prefix(&prefix).map(str::to_string))
            .collect()
    }
}

impl Telemetry for V1 {
    fn record_build_cache_error(&self, label: &str, _value: String) {
        self.0.lock().push(format!("build_cache_error:{label}"));
    }
    fn record_client_error(&self, label: &str, _value: String) {
        self.0.lock().push(format!("client_error:{label}"));
    }
    fn record_client_operation_total(&self, _label: &str) {}
    fn record_deserialization_error(&self, label: &str, _value: String) {
        self.0.lock().push(format!("deserialization_error:{label}"));
    }
    fn record_http_cache_outcome(&self, _label: &str, _value: String) {}
}

struct Harness {
    capture: Arc<Capture>,
    v1: V1,
    _restore: sink::Restore,
    _lock: parking_lot::MutexGuard<'static, ()>,
}

fn harness() -> Harness {
    let lock = super::test_lock();
    viaduct_dev::init_backend_dev();
    report::reset();
    let capture = Arc::new(Capture::default());
    let restore = sink::replace(capture.clone());
    let v1 = V1::default();
    super::install(v1.clone());
    Harness {
        capture,
        v1,
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

fn mock_ads(status: usize, body: String) -> mockito::Mock {
    mock("POST", format!("{BASE}/ads").as_str())
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create()
}

#[test]
fn test_successful_request_reports_nothing() {
    let h = harness();
    let m = mock_ads(200, happy_body().to_string());

    client(None).request_image_ads(placement(), None).unwrap();
    m.assert();

    assert!(h.capture.error_reports.lock().is_empty());
    assert!(h.v1.recorded("client_error").is_empty());
}

#[test]
fn test_expected_errors_stay_in_v1_metrics() {
    let h = harness();
    let _m = mock_ads(503, "upstream unavailable".into());

    assert!(client(None).request_image_ads(placement(), None).is_err());

    assert_eq!(h.v1.recorded("client_error"), vec!["request_ads"]);
    assert!(h.capture.error_reports.lock().is_empty());
}

#[test]
fn test_unexpected_errors_move_to_the_ping() {
    let h = harness();
    let _m = mock_ads(400, "context_id=secret".into());

    assert!(client(None).request_image_ads(placement(), None).is_err());

    assert!(h.v1.recorded("client_error").is_empty());
    let reports = h.capture.error_reports.lock().clone();
    assert_eq!(reports.len(), 1);
    let record = &reports[0].0["records"][0];
    assert_eq!(record["kind"], "http_bad_request");
    assert_eq!(record["op"], "request_image_ads");
    assert_eq!(record["stage"], "request");
    assert_eq!(record["fatal"], true);
    assert!(!reports[0].0.to_string().contains("secret"));
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
    let _m = mock_ads(200, body.to_string());

    client(None).request_image_ads(placement(), None).unwrap();

    assert!(h.v1.recorded("deserialization_error").is_empty());
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
    let gone = mock("GET", format!("{BASE}/click").as_str())
        .with_status(404)
        .create();
    let url = format!("{}{BASE}/click", mockito::server_url());

    // A callback MARS no longer knows is expected.
    assert!(client.record_click(url, None).is_err());
    gone.assert();
    assert_eq!(h.v1.recorded("client_error"), vec!["record_click"]);
    assert!(h.capture.error_reports.lock().is_empty());

    // A callback URL that does not parse is a caller bug.
    assert!(client.record_click("not a url".into(), None).is_err());
    assert_eq!(h.v1.recorded("client_error"), vec!["record_click"]);
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
    assert!(h.v1.recorded("build_cache_error").is_empty());

    client.clear_cache().unwrap();

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
fn test_builder_installs_the_sink_until_shutdown() {
    use crate::MozAdsTelemetrySink;

    struct RecordingSink(Arc<Mutex<Vec<String>>>);

    impl MozAdsTelemetrySink for RecordingSink {
        fn submit_error_reports(&self, json: String, _reason: String) {
            self.0.lock().push(json);
        }
    }

    let _lock = super::test_lock();
    report::reset();
    let _restore = sink::replace_with_none();
    let reports = Arc::new(Mutex::new(vec![]));
    let client = Arc::new(MozAdsClientBuilder::new())
        .telemetry_sink(Box::new(RecordingSink(reports.clone())))
        .build();

    client.record_click("not a url".into(), None).unwrap_err();
    assert_eq!(reports.lock().len(), 1);

    client.shutdown().unwrap();
    assert!(sink::sink().is_none());
}

#[test]
fn test_recorded_values_are_documented() {
    use super::classify::{ErrorKind, Tier};
    use super::scope::{Operation, Stage};

    let docs = include_str!("../../docs/telemetry.md");
    let names = ErrorKind::ALL
        .iter()
        .filter(|k| k.tier() == Tier::Unexpected)
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
