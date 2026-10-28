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

use self::{
    ad_request::AdRequest,
    ad_response::AdResponse,
    error::{
        CallbackRequestError, FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError,
    },
    preflight::{PreflightRequest, PreflightResponse},
    transport::MARSTransport,
};
use crate::cache_control::CacheControl;
use url::Url;
use viaduct::{Headers, Request};

/// A deserialized MARS response, with the `Cache-Control` it was sent with.
pub struct MARSResponse<T> {
    pub cache_control: CacheControl,
    pub value: T,
}

/// Sends MARS requests and deserializes their responses. Caching is up to the
/// caller.
pub struct MARSClient {
    environment: Environment,
    transport: MARSTransport,
}

impl MARSClient {
    pub fn new(environment: Environment) -> Self {
        Self {
            environment,
            transport: MARSTransport,
        }
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn fetch_ads(
        &self,
        ad_request: AdRequest,
    ) -> Result<MARSResponse<AdResponse>, FetchAdsError> {
        let ohttp = ad_request.ohttp;
        let response = self.transport.send(ad_request, ohttp)?;
        Ok(MARSResponse {
            cache_control: CacheControl::from(&response),
            value: response.json()?,
        })
    }

    pub fn fetch_preflight(
        &self,
        request: PreflightRequest,
    ) -> Result<MARSResponse<PreflightResponse>, CallbackRequestError> {
        let response = self.transport.send(request, false)?;
        Ok(MARSResponse {
            cache_control: CacheControl::from(&response),
            value: response.json()?,
        })
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

    fn make_test_client() -> MARSClient {
        MARSClient::new(Environment::Test)
    }

    #[test]
    fn test_record_impression_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/impression_callback_url")
            .with_status(200)
            .create();
        let client = make_test_client();
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

        let client = make_test_client();
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

        let client = make_test_client();
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

        let client = make_test_client();
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
            .with_header("cache-control", "max-age=60")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let client = make_test_client();
        let ad_request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            Environment::Test,
            AdRequestFlags::default(),
            false,
            make_happy_placement_requests(),
            AdRequestRelevance::default(),
        )
        .unwrap();

        let response = client.fetch_ads(ad_request).unwrap();
        assert_eq!(expected_response, response.value);
        assert_eq!(response.cache_control.max_age, Some(60));
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

        let client = make_test_client();
        let preflight = client
            .fetch_preflight(PreflightRequest::new(&Environment::Test))
            .unwrap();
        assert_eq!(preflight.value.geo_location, "US-CA");
        assert_eq!(preflight.value.normalized_ua, "Firefox/140.0");
        m.assert();
    }
}
