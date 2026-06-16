/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
use parking_lot::Mutex;
use url::Url as AdsClientUrl;

use client::AdsClient;
use http_cache::CachePolicy;
use mars::ad_request::AdPlacementRequest;
use worker::Command;

mod client;
mod ffi;
pub mod http_cache;
mod mars;
pub mod telemetry;
mod worker;

pub use ffi::*;

use crate::ffi::telemetry::MozAdsTelemetryWrapper;

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
    inner: Arc<Mutex<AdsClient<MozAdsTelemetryWrapper>>>,
    command_tx: Mutex<Sender<Command>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    next_sub_id: AtomicU64,
}

impl MozAdsClient {
    /// Build a client around an already-constructed `AdsClient` and start its
    /// background worker thread.
    pub(crate) fn spawn(inner: Arc<Mutex<AdsClient<MozAdsTelemetryWrapper>>>) -> Self {
        let (command_tx, handle) = worker::spawn(inner.clone());
        Self {
            inner,
            command_tx: Mutex::new(command_tx),
            worker: Mutex::new(Some(handle)),
            next_sub_id: AtomicU64::new(0),
        }
    }

    fn subscribe(&self, placement_id: String, sink: worker::Sink) -> Arc<MozAdsSubscription> {
        let id = self.next_sub_id.fetch_add(1, Ordering::Relaxed);
        let command_tx = self.command_tx.lock().clone();
        let _ = command_tx.send(Command::Subscribe {
            id,
            placement_id,
            sink,
        });
        Arc::new(MozAdsSubscription::new(id, command_tx))
    }
}

impl Drop for MozAdsClient {
    fn drop(&mut self) {
        let _ = self.command_tx.lock().send(Command::Shutdown);
        if let Some(handle) = self.worker.lock().take() {
            let _ = handle.join();
        }
    }
}

#[uniffi::export]
impl MozAdsClient {
    /// Subscribe to a placement's tile ad and receive a live stream of updates.
    ///
    /// The returned [`MozAdsSubscription`] is the teardown handle the native
    /// `Flow` / `AsyncStream` wrappers drive; surfaces use those wrappers rather
    /// than calling this directly.
    pub fn subscribe_tile_ad(
        &self,
        placement_id: String,
        subscriber: Arc<dyn MozAdsTileSubscriber>,
    ) -> Arc<MozAdsSubscription> {
        self.subscribe(placement_id, worker::Sink::Tile(subscriber))
    }

    /// Subscribe to a placement's image ad. See [`Self::subscribe_tile_ad`].
    pub fn subscribe_image_ad(
        &self,
        placement_id: String,
        subscriber: Arc<dyn MozAdsImageSubscriber>,
    ) -> Arc<MozAdsSubscription> {
        self.subscribe(placement_id, worker::Sink::Image(subscriber))
    }

    /// Subscribe to a placement's spoc ads. See [`Self::subscribe_tile_ad`].
    pub fn subscribe_spoc_ads(
        &self,
        placement_id: String,
        subscriber: Arc<dyn MozAdsSpocSubscriber>,
    ) -> Arc<MozAdsSubscription> {
        self.subscribe(placement_id, worker::Sink::Spoc(subscriber))
    }

    pub fn clear_cache(&self) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .clear_cache()
            .map_err(|e| MozAdsClientApiError::Other {
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
        let url = AdsClientUrl::parse(&click_url)
            .map_err(|e| ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let inner = self.inner.lock();
        inner
            .record_click(url, ohttp)
            .map_err(ComponentError::RecordClick)
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn record_impression(
        &self,
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&impression_url).map_err(|e| {
            ComponentError::RecordImpression(CallbackRequestError::InvalidUrl(e).into())
        })?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let inner = self.inner.lock();
        inner
            .record_impression(url, ohttp)
            .map_err(ComponentError::RecordImpression)
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn report_ad(
        &self,
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
    ) -> AdsClientApiResult<()> {
        let url = AdsClientUrl::parse(&report_url)
            .map_err(|e| ComponentError::ReportAd(CallbackRequestError::InvalidUrl(e).into()))?;
        let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
        let inner = self.inner.lock();
        inner
            .report_ad(url, reason.into(), ohttp)
            .map_err(ComponentError::ReportAd)
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_image_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, MozAdsImage>> {
        let inner = self.inner.lock();
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let ohttp = options.as_ref().map(|o| o.ohttp).unwrap_or(false);
        let cache_policy: CachePolicy = options.into();
        let response = inner
            .request_image_ads(requests, Some(cache_policy), ohttp)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }

    #[handle_error(ComponentError)]
    #[uniffi::method(default(options = None))]
    pub fn request_spoc_ads(
        &self,
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
    ) -> AdsClientApiResult<HashMap<String, Vec<MozAdsSpoc>>> {
        let inner = self.inner.lock();
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let ohttp = options.as_ref().map(|o| o.ohttp).unwrap_or(false);
        let cache_policy: CachePolicy = options.into();
        let response = inner
            .request_spoc_ads(requests, Some(cache_policy), ohttp)
            .map_err(ComponentError::RequestAds)?;
        Ok(response
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
        let inner = self.inner.lock();
        let requests: Vec<AdPlacementRequest> = moz_ad_requests.iter().map(|r| r.into()).collect();
        let ohttp = options.as_ref().map(|o| o.ohttp).unwrap_or(false);
        let cache_policy: CachePolicy = options.into();
        let response = inner
            .request_tile_ads(requests, Some(cache_policy), ohttp)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}
