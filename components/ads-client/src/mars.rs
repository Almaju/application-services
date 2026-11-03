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
    preflight::PreflightResponse,
    transport::MARSTransport,
};
use crate::http_cache::CacheControl;
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use url::Url;
use viaduct::{Headers, Request};

/// How long the preflight is kept when its response has no `max-age`.
const PREFLIGHT_DEFAULT_TTL: Duration = Duration::from_secs(300);

pub(crate) struct MARSClient {
    environment: Environment,
    /// The last preflight, until it expires.
    preflight: Mutex<Option<(PreflightResponse, Instant)>>,
    transport: MARSTransport,
}

impl MARSClient {
    pub(crate) fn new(environment: Environment) -> Self {
        Self {
            environment,
            preflight: Mutex::new(None),
            transport: MARSTransport,
        }
    }

    pub(crate) fn environment(&self) -> &Environment {
        &self.environment
    }

    /// Sends `ad_request` and returns the ads with the response's
    /// `Cache-Control`.
    pub(crate) fn fetch_ads(
        &self,
        mut ad_request: AdRequest,
    ) -> Result<(AdResponse, CacheControl), FetchAdsError> {
        let ohttp = ad_request.ohttp;
        if ohttp {
            ad_request
                .headers
                .extend(Headers::try_from(self.fetch_preflight()?)?);
        }

        let response = self.transport.send(ad_request, ohttp)?;
        Ok((response.json()?, CacheControl::from(&response)))
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

    /// The preflight, kept in memory for as long as its `Cache-Control` allows.
    fn fetch_preflight(&self) -> Result<PreflightResponse, CallbackRequestError> {
        let mut cached = self.preflight.lock();
        if let Some((preflight, expires_at)) = cached.as_ref() {
            if Instant::now() < *expires_at {
                return Ok(preflight.clone());
            }
        }

        let response = self.transport.send(
            Request::get(self.environment.clone().into_url("ads-preflight")),
            false,
        )?;
        let preflight: PreflightResponse = response.json()?;
        let cache_control = CacheControl::from(&response);
        let ttl = cache_control
            .max_age_duration()
            .unwrap_or(PREFLIGHT_DEFAULT_TTL);
        *cached = Instant::now()
            .checked_add(ttl)
            .filter(|_| cache_control.should_cache())
            .map(|expires_at| (preflight.clone(), expires_at));
        Ok(preflight)
    }

    fn make_callback_request(
        &self,
        callback: Url,
        ohttp: bool,
    ) -> Result<(), CallbackRequestError> {
        let mut request = Request::get(callback);
        if ohttp {
            request
                .headers
                .extend(Headers::try_from(self.fetch_preflight()?)?);
        }
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
        let (response, cache_control) = client.fetch_ads(ad_request).unwrap();
        assert_eq!(expected_response, response);
        assert_eq!(cache_control.max_age, Some(60));
        m.assert();
    }

    #[test]
    fn test_preflight_is_kept_until_it_expires() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/ads-preflight")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "max-age=60")
            .with_body(r#"{"geo_location":"US-CA","normalized_ua":"Firefox/140.0"}"#)
            .expect(1)
            .create();

        let client = make_test_client();
        let first = client.fetch_preflight().unwrap();
        let second = client.fetch_preflight().unwrap();
        assert_eq!(first.geo_location, "US-CA");
        assert_eq!(second.geo_location, "US-CA");
        m.assert();
    }
}
