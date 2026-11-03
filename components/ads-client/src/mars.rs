/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub(crate) mod ad_request;
pub(crate) mod ad_response;
pub(crate) mod environment;
pub(crate) mod error;
mod preflight;
pub(crate) mod report_reason;
mod transport;

pub(crate) use environment::Environment;
pub(crate) use report_reason::ReportReason;

use self::{
    ad_request::AdRequest,
    ad_response::AdResponse,
    error::{
        CallbackRequestError, FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError,
    },
    transport::MARSTransport,
};
use crate::cache_control::CacheControl;
use url::Url;
use viaduct::Request;

/// A deserialized MARS response, with the `Cache-Control` it was sent with.
pub(crate) struct MARSResponse<T> {
    pub cache_control: CacheControl,
    pub value: T,
}

/// Serializes MARS requests and deserializes their responses. Sending them,
/// OHTTP included, is up to its transport, and caching is up to the caller.
pub(crate) struct MARSClient {
    environment: Environment,
    transport: MARSTransport,
}

impl MARSClient {
    pub(crate) fn new(environment: Environment) -> Self {
        let transport = MARSTransport::new(&environment);
        Self {
            environment,
            transport,
        }
    }

    pub(crate) fn environment(&self) -> &Environment {
        &self.environment
    }

    pub(crate) fn fetch_ads(
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

    pub(crate) fn record_click(&self, callback: Url, ohttp: bool) -> Result<(), RecordClickError> {
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    pub(crate) fn record_impression(
        &self,
        callback: Url,
        ohttp: bool,
    ) -> Result<(), RecordImpressionError> {
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    pub(crate) fn report_ad(
        &self,
        mut callback: Url,
        reason: ReportReason,
        ohttp: bool,
    ) -> Result<(), ReportAdError> {
        callback
            .query_pairs_mut()
            .append_pair("reason", reason.as_str());
        Ok(self.make_callback_request(callback, ohttp)?)
    }

    fn make_callback_request(
        &self,
        callback: Url,
        ohttp: bool,
    ) -> Result<(), CallbackRequestError> {
        self.transport.send(Request::get(callback), ohttp)?;
        Ok(())
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
        let result = client.record_impression(url, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_click_with_valid_url_should_succeed() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/click_callback_url").with_status(200).create();

        let client = make_test_client();
        let url = Url::parse(&format!("{}/click_callback_url", &mockito::server_url())).unwrap();
        let result = client.record_click(url, false);
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
        let result = client.report_ad(url, ReportReason::NotInterested, false);
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
}
