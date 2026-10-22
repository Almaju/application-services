/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
use url::Url as AdsClientUrl;

use client::AdsClient;
use http_cache::CachePolicy;
use mars::ad_request::{AdPlacementRequest, AdRequestFlags};
use telemetry::Operation;
mod ads;
#[cfg(feature = "stateful")]
pub mod ads_store;
pub mod bytesize;
mod client;
pub mod clock;
mod ffi;
pub mod http_cache;
mod mars;
#[cfg(feature = "stateful")]
pub mod request;
pub mod telemetry;

pub use ffi::*;

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_string(),
});

#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: AdsClient,
}

#[uniffi::export]
impl MozAdsClient {
    pub fn clear_cache(&self) -> AdsClientApiResult<()> {
        let scope = telemetry::begin(Operation::ClearCache);
        let result = self.inner.clear_cache();
        scope.finish(&result);
        result.map_err(|e| MozAdsClientApiError::Other {
            reason: format!("Failed to clear cache: {}", e),
        })
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_click(
        &self,
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let scope = telemetry::begin(Operation::RecordClick);
        let result = AdsClientUrl::parse(&click_url)
            .map_err(|e| ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into()))
            .and_then(|url| {
                self.inner
                    .record_click(url, ohttp)
                    .map_err(ComponentError::RecordClick)
            });
        scope.finish(&result);
        result
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_impression(
        &self,
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let scope = telemetry::begin(Operation::RecordImpression);
        let result = AdsClientUrl::parse(&impression_url)
            .map_err(|e| {
                ComponentError::RecordImpression(CallbackRequestError::InvalidUrl(e).into())
            })
            .and_then(|url| {
                self.inner
                    .record_impression(url, ohttp)
                    .map_err(ComponentError::RecordImpression)
            });
        scope.finish(&result);
        result
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn report_ad(
        &self,
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let scope = telemetry::begin(Operation::ReportAd);
        let result = AdsClientUrl::parse(&report_url)
            .map_err(|e| ComponentError::ReportAd(CallbackRequestError::InvalidUrl(e).into()))
            .and_then(|url| {
                self.inner
                    .report_ad(url, reason.into(), ohttp)
                    .map_err(ComponentError::ReportAd)
            });
        scope.finish(&result);
        result
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsImage>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let scope = telemetry::begin(Operation::RequestImageAds);
        let result = self
            .inner
            .request_image_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds);
        scope.finish(&result);
        Ok(result?.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_spoc_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAdsSpoc>>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let scope = telemetry::begin(Operation::RequestSpocAds);
        let result = self
            .inner
            .request_spoc_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds);
        scope.finish(&result);
        Ok(result?
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|spoc| spoc.into()).collect()))
            .collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_spoc_or_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAdsImageOrSpoc>>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let scope = telemetry::begin(Operation::RequestSpocOrImageAds);
        let result = self
            .inner
            .request_spoc_or_image_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds);
        scope.finish(&result);
        Ok(result?
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|spoc| spoc.into()).collect()))
            .collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_tile_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsTile>> {
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let relevance = options.relevance;
        let scope = telemetry::begin(Operation::RequestTileAds);
        let result = self
            .inner
            .request_tile_ads(requests, flags, cache_policy, ohttp, blocks, relevance)
            .map_err(ComponentError::RequestAds);
        scope.finish(&result);
        Ok(result?.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    // Allows the ads-client to unload some references and prepare for a safe shutdown.
    // Other methods should not be called after this one.
    // Currently, we shutdown (with no expected errors).
    // However, future shutdowns may require an error to be logged, so we keep the Result.
    #[uniffi::method()]
    pub fn shutdown(&self) -> AdsClientApiResult<()> {
        self.inner.shutdown();
        Ok(())
    }
}
