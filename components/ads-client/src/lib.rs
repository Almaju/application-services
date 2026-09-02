/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;
use std::time::Duration;

use client::error::ComponentError;
use error_support::handle_error;
use mars::error::CallbackRequestError;
use parking_lot::Mutex;
use url::Url as AdsClientUrl;

use client::AdsClient;
use error_support::error;
use http_cache::CachePolicy;
use mars::ad_request::{AdPlacementRequest, AdRequestFlags};
mod client;
mod ffi;
pub mod http_cache;
mod mars;
pub mod telemetry;

pub use ffi::*;

use crate::ffi::telemetry::MozAdsTelemetryWrapper;
use crate::ffi::MozAdsContextIdProviderWrapper;
use crate::telemetry::Telemetry;

#[cfg(test)]
mod test_utils;

uniffi::setup_scaffolding!("ads_client");

uniffi::custom_type!(AdsClientUrl, String, {
    remote,
    try_lift: |val| Ok(AdsClientUrl::parse(&val)?),
    lower: |obj| obj.as_str().to_string(),
});

/// How long `shutdown` is willing to wait for the cache database.
///
/// Only the database close needs the `inner` lock, and failing to close it is benign:
/// SQLite recovers an unclosed connection on next open. Blocking browser shutdown is
/// not benign, so this stays short.
const DB_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(uniffi::Object)]
pub struct MozAdsClient {
    inner: Mutex<AdsClient<MozAdsTelemetryWrapper>>,
    /// Handles to the foreign callback references also held by `inner`.
    ///
    /// These sit outside the mutex on purpose. Every other method holds `inner` across
    /// blocking network I/O, so anything that needs the lock can be stalled for as long
    /// as a request takes. Releasing foreign references is the one thing that must not
    /// be stalled — see [MozAdsClient::shutdown].
    telemetry: MozAdsTelemetryWrapper,
    context_id_provider: Option<MozAdsContextIdProviderWrapper>,
}

#[uniffi::export]
impl MozAdsClient {
    pub fn clear_cache(&self) -> AdsClientApiResult<()> {
        let inner = self.inner.lock();
        inner
            .clear_cache()
            .map_err(|e| MozAdsClientApiError::Other {
                reason: format!("Failed to clear cache: {}", e),
            })
    }

    /// Release foreign references and prepare for a safe shutdown.
    ///
    /// Other methods should not be called after this one.
    ///
    /// This runs in two phases, because the two things being shut down have different
    /// urgency and different locking needs:
    ///
    /// 1. Foreign (JS/Kotlin/Swift) callback references. These are what cause the
    ///    shutdown crash: UniFFI releases a foreign object only once every Rust
    ///    reference is dropped, and the foreign side may tear down its handle map
    ///    before that happens. `shutdown` is the only deterministic release point we
    ///    have — dropping the client is not, since that waits on foreign GC. So this
    ///    phase must not be able to block, and takes no lock.
    ///
    /// 2. The cache database. Closing it needs `&mut AdsClient` and therefore the
    ///    `inner` lock, which an in-flight request holds across blocking network I/O.
    ///    Bounded by [DB_SHUTDOWN_TIMEOUT] so a slow or hung request cannot keep the
    ///    browser from quitting.
    ///
    /// Doing these in one phase under one lock — as this used to — means a request in
    /// flight delays the release in phase 1, and shutdown can be forced through while
    /// the foreign references are still held.
    #[uniffi::method()]
    pub fn shutdown(&self) -> AdsClientApiResult<()> {
        // Phase 1: cannot block.
        self.telemetry.shutdown();
        if let Some(context_id_provider) = &self.context_id_provider {
            context_id_provider.shutdown();
        }

        // Phase 2: best-effort. `shutdown_client` repeats phase 1, which is harmless —
        // both releases are idempotent.
        match self.inner.try_lock_for(DB_SHUTDOWN_TIMEOUT) {
            Some(mut inner) => {
                if let Err(err) = inner.shutdown_client() {
                    // Log the error, but continue with shutdown.
                    error!("Failed to shutdown the ads client: {:?}", err);
                }
            }
            None => {
                error!(
                    "Timed out waiting on an in-flight request to close the ads-client \
                     cache database; skipping the close. Foreign references have already \
                     been released."
                );
            }
        }
        Ok(())
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
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = inner
            .request_image_ads(requests, flags, cache_policy, ohttp, blocks)
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
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = inner
            .request_spoc_ads(requests, flags, cache_policy, ohttp, blocks)
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
        let options = options.unwrap_or_default();
        let flags = AdRequestFlags::from(&options);
        let ohttp = options.ohttp;
        let cache_policy = options.cache_policy.map(CachePolicy::from);
        let blocks = options.blocks;
        let response = inner
            .request_tile_ads(requests, flags, cache_policy, ohttp, blocks)
            .map_err(ComponentError::RequestAds)?;
        Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::telemetry::NoopMozAdsTelemetry;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Instant;

    /// Build a client whose telemetry stands in for a foreign implementation, returning
    /// the client alongside a `Weak` to the reference we expect `shutdown` to release.
    /// The builder is returned too, so callers can keep it alive the way foreign code
    /// does while waiting for GC.
    fn client_and_telemetry_ref() -> (
        MozAdsClient,
        Arc<MozAdsClientBuilder>,
        std::sync::Weak<dyn MozAdsTelemetry>,
    ) {
        let builder = Arc::new(MozAdsClientBuilder::new())
            .environment(MozAdsEnvironment::Test)
            .telemetry(Box::new(NoopMozAdsTelemetry));
        let client = builder.build();
        let weak = {
            let strong = client
                .telemetry
                .clone_inner_arc()
                .expect("telemetry should be set before shutdown");
            Arc::downgrade(&strong)
        };
        assert_ne!(weak.strong_count(), 0);
        (client, builder, weak)
    }

    /// `build` must move the foreign reference out of the builder, not clone it.
    ///
    /// Foreign code holds the builder until its GC gets around to it, which may be
    /// after browser shutdown. A clone left behind here would keep the foreign object
    /// alive no matter what the client does.
    #[test]
    fn test_build_moves_foreign_refs_out_of_builder() {
        viaduct_dev::init_backend_dev();

        let (client, builder, weak) = client_and_telemetry_ref();
        client.shutdown().unwrap();

        // The builder is deliberately still alive at this point.
        assert_eq!(
            weak.strong_count(),
            0,
            "builder is still holding the foreign telemetry reference after build()"
        );
        drop(builder);
    }

    /// `shutdown` must release foreign references even while a request holds `inner`.
    ///
    /// This is the shutdown-crash race: requests hold the lock across blocking network
    /// I/O, so a `shutdown` that waits for the lock can be forced through by the
    /// browser with the foreign references still held.
    #[test]
    fn test_shutdown_releases_foreign_refs_while_request_in_flight() {
        viaduct_dev::init_backend_dev();

        let (client, _builder, weak) = client_and_telemetry_ref();
        let client = Arc::new(client);

        // Stand in for a request blocked on the network with `inner` held.
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let holder = {
            let client = Arc::clone(&client);
            std::thread::spawn(move || {
                let _guard = client.inner.lock();
                locked_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        };
        locked_rx.recv().unwrap();

        let started = Instant::now();
        client.shutdown().unwrap();
        let elapsed = started.elapsed();

        assert_eq!(
            weak.strong_count(),
            0,
            "foreign telemetry reference was not released while a request held the lock"
        );
        assert!(
            elapsed < DB_SHUTDOWN_TIMEOUT * 4,
            "shutdown blocked on the in-flight request for {elapsed:?}"
        );

        release_tx.send(()).unwrap();
        holder.join().unwrap();
    }
}
