/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod ad_request;
pub mod ad_response;
pub mod environment;
pub mod error;
pub mod preflight;
pub mod report_reason;
mod transport;

pub use environment::Environment;
pub use report_reason::ReportReason;
use sql_support::open_database;

use self::{
    ad_request::AdRequest,
    ad_response::AdResponse,
    error::{
        CallbackRequestError, FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError,
    },
    preflight::{PreflightRequest, PreflightResponse},
    transport::MARSTransport,
};
use crate::{
    http_cache::{HttpCache, RequestHash},
    CachePolicy,
};
use url::Url;
use viaduct::{Headers, Request};

pub struct MARSClient {
    environment: Environment,
    transport: MARSTransport,
}

impl MARSClient {
    pub fn new(environment: Environment, http_cache: Option<HttpCache>) -> Self {
        Self {
            environment,
            transport: MARSTransport::new(http_cache),
        }
    }

    pub fn clear_cache(&self) -> Result<(), open_database::Error> {
        self.transport.clear_cache()
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn fetch_ads(
        &self,
        ad_request: AdRequest,
        cache_policy: CachePolicy,
    ) -> Result<AdResponse, FetchAdsError> {
        let ohttp = ad_request.ohttp;
        Ok(self.transport.send(ad_request, &cache_policy, ohttp)?)
    }

    pub fn fetch_preflight(
        &self,
        request: PreflightRequest,
    ) -> Result<PreflightResponse, CallbackRequestError> {
        Ok(self
            .transport
            .send(request, &CachePolicy::CacheFirst { ttl: None }, false)?)
    }

    // TODO: Remove this allow(dead_code) when cache invalidation is re-enabled behind Nimbus experiment
    #[allow(dead_code)]
    pub fn invalidate_cache_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), open_database::Error> {
        self.transport.invalidate_cache_by_hash(request_hash)
    }

    pub fn record_click(
        &self,
        callback: Url,
        ohttp: bool,
        headers: Headers,
    ) -> Result<(), RecordClickError> {
        Ok(self.make_callback_request(callback, ohttp, headers)?)
    }

    pub fn record_impression(
        &self,
        callback: Url,
        ohttp: bool,
        headers: Headers,
    ) -> Result<(), RecordImpressionError> {
        Ok(self.make_callback_request(callback, ohttp, headers)?)
    }

    pub fn report_ad(
        &self,
        mut callback: Url,
        reason: ReportReason,
        ohttp: bool,
        headers: Headers,
    ) -> Result<(), ReportAdError> {
        callback
            .query_pairs_mut()
            .append_pair("reason", reason.as_str());
        Ok(self.make_callback_request(callback, ohttp, headers)?)
    }

    #[allow(dead_code)]
    pub fn shutdown_db(&self) {
        self.transport.shutdown_db();
    }

    fn make_callback_request(
        &self,
        callback: Url,
        ohttp: bool,
        headers: Headers,
    ) -> Result<(), CallbackRequestError> {
        let mut request = Request::get(callback);
        request.headers.extend(headers);
        self.transport.fire(request, ohttp).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::mars::ad_request::{AdRequestFlags, AdRequestRelevance};
    use crate::test_utils::{
        get_example_happy_image_response, make_happy_placement_requests, TEST_CONTEXT_ID,
    };
    use mockito::mock;

    fn make_test_client(http_cache: Option<HttpCache>) -> MARSClient {
        MARSClient::new(Environment::Test, http_cache)
    }

    fn make_ad_request() -> AdRequest {
        AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            Environment::Test,
            AdRequestFlags::default(),
            false,
            make_happy_placement_requests(),
            AdRequestRelevance::default(),
        )
        .unwrap()
    }

    #[test]
    fn test_record_impression_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/impression_callback_url")
            .with_status(200)
            .create();
        let client = make_test_client(None);
        let url = Url::parse(&format!(
            "{}/impression_callback_url",
            &mockito::server_url()
        ))
        .unwrap();
        let result = client.record_impression(url, false, Headers::new());
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_click_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/click_callback_url").with_status(200).create();

        let client = make_test_client(None);
        let url = Url::parse(&format!("{}/click_callback_url", &mockito::server_url())).unwrap();
        let result = client.record_click(url, false, Headers::new());
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_callback_request_sends_given_headers() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/click_with_headers")
            .match_header("X-Geo-Location", "US-CA")
            .with_status(200)
            .create();

        let client = make_test_client(None);
        let url = Url::parse(&format!("{}/click_with_headers", &mockito::server_url())).unwrap();
        let mut headers = Headers::new();
        headers.insert("X-Geo-Location", "US-CA").unwrap();
        let result = client.record_click(url, false, headers);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_report_ad_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/report_ad_callback_url")
            .match_query(mockito::Matcher::UrlEncoded(
                "reason".into(),
                "not_interested".into(),
            ))
            .with_status(200)
            .create();

        let client = make_test_client(None);
        let url = Url::parse(&format!(
            "{}/report_ad_callback_url",
            &mockito::server_url()
        ))
        .unwrap();
        let result = client.report_ad(url, ReportReason::NotInterested, false, Headers::new());
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_fetch_ads_success() {
        viaduct_dev::init_backend_dev();
        let expected_response = get_example_happy_image_response();

        let m = mock("POST", "/ads")
            .match_header("content-type", "application/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let client = make_test_client(None);

        let response = client
            .fetch_ads(make_ad_request(), CachePolicy::default())
            .unwrap();
        assert_eq!(expected_response, response);
        m.assert();
    }

    #[test]
    fn test_fetch_preflight_success() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/ads-preflight")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"geo_location":"US-CA","normalized_ua":"Firefox/140.0"}"#)
            .create();

        let client = make_test_client(None);
        let preflight = client
            .fetch_preflight(PreflightRequest::new(&Environment::Test))
            .unwrap();
        assert_eq!(preflight.geo_location, "US-CA");
        assert_eq!(preflight.normalized_ua, "Firefox/140.0");
        m.assert();
    }

    #[test]
    fn test_fetch_ads_cache_hit_skips_network() {
        viaduct_dev::init_backend_dev();
        let expected = get_example_happy_image_response();
        let m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected.data).unwrap())
            .expect(1) // only first request goes to network
            .create();

        let cache = HttpCache::builder("test_fetch_ads_cache_hit_skips_network.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
            .build()
            .unwrap();
        let client = make_test_client(Some(cache));

        // First call should be a miss then warm the cache
        let response1 = client
            .fetch_ads(make_ad_request(), CachePolicy::default())
            .unwrap();
        assert_eq!(response1, expected);

        // Second call should be a hit
        let response2 = client
            .fetch_ads(make_ad_request(), CachePolicy::default())
            .unwrap();
        assert_eq!(response2, expected);
        m.assert();
    }

    #[test]
    fn test_record_click_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_click.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
            .build()
            .unwrap();

        let client = make_test_client(Some(cache));
        let callback_url = Url::parse(&format!("{}/click", mockito::server_url())).unwrap();

        let m = mock("GET", "/click").with_status(200).create();

        let result = client.record_click(callback_url, false, Headers::new());
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_impression_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_impression.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
            .build()
            .unwrap();

        let client = make_test_client(Some(cache));
        let callback_url = Url::parse(&format!("{}/impression", mockito::server_url())).unwrap();

        let m = mock("GET", "/impression").with_status(200).create();

        let result = client.record_impression(callback_url, false, Headers::new());
        assert!(result.is_ok());
        m.assert();
    }
}
