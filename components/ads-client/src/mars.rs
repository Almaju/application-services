/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The MARS API layer: a client with one method per operation of
//! <https://ads.mozilla.org/assets/docs/openapi/mars-api.html>, and the
//! schemas those operations send and receive.

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
    ad_request::{AdRequest, GetAdsRequest},
    ad_response::AdResponse,
    error::{CallbackRequestError, FetchAdsError},
    preflight::{PreflightRequest, PreflightResponse},
    transport::MARSTransport,
};
use crate::{
    http_cache::{HttpCache, RequestHash},
    CachePolicy,
};
use url::Url;
use viaduct::{Headers, Request};

/// Sends MARS what it is given and returns what MARS answers.
///
/// Deciding what to ask for and what to keep of the answer (context IDs, OHTTP
/// preflights, one kind of ad per request, tagged callbacks) is up to the
/// caller, `AdsClient`.
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

    /// `POST /v1/ads` (`getAds`).
    ///
    /// `preflight` fills the `X-Geo-Location` and `X-User-Agent` headers: OHTTP
    /// requests must carry them, other requests must not.
    ///
    /// Also returns the hash the HTTP cache keys the response by.
    pub fn fetch_ads(
        &self,
        ad_request: &AdRequest,
        preflight: Option<PreflightResponse>,
        cache_policy: &CachePolicy,
        ohttp: bool,
    ) -> Result<(AdResponse, RequestHash), FetchAdsError> {
        let request = GetAdsRequest {
            body: ad_request,
            environment: &self.environment,
            headers: preflight
                .map(Headers::try_from)
                .transpose()?
                .unwrap_or_default(),
            ohttp,
        };
        let request_hash = RequestHash::new(&request);
        let response = self.transport.send(request, cache_policy, ohttp)?;
        Ok((response.json()?, request_hash))
    }

    /// `GET /v1/ads-preflight`.
    pub fn fetch_preflight(&self) -> Result<PreflightResponse, CallbackRequestError> {
        let response = self.transport.send(
            PreflightRequest(self.environment.clone().into_url("ads-preflight")),
            &CachePolicy::CacheFirst { ttl: None },
            false,
        )?;
        Ok(response.json()?)
    }

    // TODO: Remove this allow(dead_code) when cache invalidation is re-enabled behind Nimbus experiment
    #[allow(dead_code)]
    pub fn invalidate_cache_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), open_database::Error> {
        self.transport.invalidate_cache_by_hash(request_hash)
    }

    /// `GET /v1/t`: fires one of the callbacks of an ad from an [`AdResponse`].
    /// `reason` goes with the `report` callback, and only with it.
    ///
    /// `preflight` fills the same headers as for [`Self::fetch_ads`].
    pub fn record_interaction(
        &self,
        mut callback: Url,
        reason: Option<ReportReason>,
        preflight: Option<PreflightResponse>,
        ohttp: bool,
    ) -> Result<(), CallbackRequestError> {
        if let Some(reason) = reason {
            callback
                .query_pairs_mut()
                .append_pair("reason", reason.as_str());
        }
        let mut request = Request::get(callback);
        if let Some(preflight) = preflight {
            request.headers.extend(Headers::try_from(preflight)?);
        }
        self.transport.fire(request, ohttp).map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn shutdown_db(&self) {
        self.transport.shutdown_db();
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::mars::ad_request::{AdRequestFlags, AdRequestRelevance};
    use crate::test_utils::{
        get_example_happy_image_response, make_happy_placement_requests, TEST_CONTEXT_ID,
    };
    use mockito::{mock, Matcher};

    fn make_test_client(http_cache: Option<HttpCache>) -> MARSClient {
        MARSClient::new(Environment::Test, http_cache)
    }

    fn make_ad_request() -> AdRequest {
        AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            make_happy_placement_requests(),
            AdRequestRelevance::default(),
        )
        .unwrap()
    }

    #[test]
    fn test_record_interaction_with_valid_url_should_succeed() {
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
        let result = client.record_interaction(url, None, None, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_interaction_appends_reason() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/report_ad_callback_url")
            .match_query(Matcher::UrlEncoded(
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
        let result = client.record_interaction(url, Some(ReportReason::NotInterested), None, false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_interaction_sends_preflight_headers() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/click_with_preflight")
            .match_header("X-Geo-Location", "506,MA,US")
            .match_header("X-User-Agent", "Firefox/140.0")
            .with_status(200)
            .create();

        let client = make_test_client(None);
        let url = Url::parse(&format!("{}/click_with_preflight", mockito::server_url())).unwrap();
        let preflight = PreflightResponse {
            geo_location: "506,MA,US".to_string(),
            normalized_ua: "Firefox/140.0".to_string(),
        };
        let result = client.record_interaction(url, None, Some(preflight), false);
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_fetch_ads_success() {
        viaduct_dev::init_backend_dev();
        let expected_response = get_example_happy_image_response();

        let m = mock("POST", "/ads")
            .match_header("content-type", "application/json")
            .match_header("X-Geo-Location", Matcher::Missing)
            .match_header("X-User-Agent", Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let client = make_test_client(None);

        let result = client.fetch_ads(&make_ad_request(), None, &CachePolicy::default(), false);
        assert!(result.is_ok());
        let (response, _request_hash) = result.unwrap();
        assert_eq!(expected_response, response);
        m.assert();
    }

    #[test]
    fn test_fetch_ads_sends_preflight_headers() {
        viaduct_dev::init_backend_dev();
        let m = mock("POST", "/ads")
            .match_header("X-Geo-Location", "506,MA,US")
            .match_header("X-User-Agent", "Firefox/140.0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();

        let client = make_test_client(None);
        let preflight = PreflightResponse {
            geo_location: "506,MA,US".to_string(),
            normalized_ua: "Firefox/140.0".to_string(),
        };
        let result = client.fetch_ads(
            &make_ad_request(),
            Some(preflight),
            &CachePolicy::NetworkFirst { ttl: None },
            false,
        );
        assert!(result.is_ok());
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
        let (response1, _) = client
            .fetch_ads(&make_ad_request(), None, &CachePolicy::default(), false)
            .unwrap();
        assert_eq!(response1, expected);

        // Second call should be a hit
        let (response2, _) = client
            .fetch_ads(&make_ad_request(), None, &CachePolicy::default(), false)
            .unwrap();
        assert_eq!(response2, expected);
        m.assert();
    }

    #[test]
    fn test_fetch_preflight() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/ads-preflight")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"geo_location": "506,MA,US", "normalized_ua": "Firefox/140.0"}"#)
            .create();

        let client = make_test_client(None);
        let preflight = client.fetch_preflight().unwrap();
        assert_eq!(preflight.geo_location, "506,MA,US");
        assert_eq!(preflight.normalized_ua, "Firefox/140.0");
        m.assert();
    }

    #[test]
    fn test_record_interaction_with_http_cache_makes_callback_request() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_click.db")
            .default_ttl(std::time::Duration::from_secs(300))
            .max_size(crate::bytesize::ByteSize::mib(1))
            .build()
            .unwrap();

        let client = make_test_client(Some(cache));
        let callback_url = Url::parse(&format!("{}/click", mockito::server_url())).unwrap();

        let m = mock("GET", "/click").with_status(200).expect(2).create();

        // Callbacks are fired every time, never answered from the cache.
        assert!(client
            .record_interaction(callback_url.clone(), None, None, false)
            .is_ok());
        assert!(client
            .record_interaction(callback_url, None, None, false)
            .is_ok());
        m.assert();
    }
}
