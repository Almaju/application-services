/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod telemetry;

use std::sync::Arc;

use crate::ffi::telemetry::MozAdsTelemetryWrapper;
use crate::MozAdsClient;
use ads_client_core::client::ad_request::{
    AdContentCategory, AdPlacementRequest, IABContentTaxonomy,
};
use ads_client_core::client::ad_response::{
    AdCallbacks, AdImage, AdSpoc, AdTile, SpocFrequencyCaps, SpocRanking,
};
use ads_client_core::client::config::{AdsCacheConfig, AdsClientConfig, Environment};
use ads_client_core::client::AdsClient;
use ads_client_core::client::ReportReason;
use ads_client_core::error::ComponentError;
use ads_client_core::http_cache::CachePolicy;
use error_support::{ErrorHandling, GetErrorHandling};
use parking_lot::Mutex;
use url::Url;

pub type AdsClientApiResult<T> = std::result::Result<T, MozAdsClientApiError>;

pub use telemetry::MozAdsTelemetry;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MozAdsClientApiError {
    #[error("Something unexpected occurred.")]
    Other { reason: String },
}

impl From<context_id::ApiError> for MozAdsClientApiError {
    fn from(err: context_id::ApiError) -> Self {
        MozAdsClientApiError::Other {
            reason: err.to_string(),
        }
    }
}

/// Wraps `ads_client_core::error::ComponentError` so `GetErrorHandling` (a foreign
/// trait) can be implemented for it here: Rust's orphan rules forbid implementing a
/// foreign trait directly on a foreign type, and `ComponentError` now lives in
/// `ads-client-core` rather than this crate. `#[from]` lets `?` convert a
/// `ComponentError` into this wrapper automatically inside `#[handle_error(..)]`
/// functions.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub(crate) struct FfiComponentError(#[from] ComponentError);

impl GetErrorHandling for FfiComponentError {
    type ExternalError = MozAdsClientApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(MozAdsClientApiError::Other {
            reason: self.0.to_string(),
        })
    }
}

#[derive(Default, uniffi::Record)]
pub struct MozAdsRequestOptions {
    pub cache_policy: Option<MozAdsCachePolicy>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsIABContent {
    pub category_ids: Vec<String>,
    pub taxonomy: MozAdsIABContentTaxonomy,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequest {
    #[uniffi(default = None)]
    pub iab_content: Option<MozAdsIABContent>,
    pub placement_id: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct MozAdsPlacementRequestWithCount {
    pub count: u32,
    #[uniffi(default = None)]
    pub iab_content: Option<MozAdsIABContent>,
    pub placement_id: String,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[derive(uniffi::Object)]
pub struct MozAdsClientBuilder(Mutex<MozAdsClientBuilderInner>);

#[derive(Default)]
struct MozAdsClientBuilderInner {
    cache_config: Option<MozAdsCacheConfig>,
    environment: Option<MozAdsEnvironment>,
    rotation_days: Option<u8>,
    telemetry: Option<Arc<dyn MozAdsTelemetry>>,
}

impl Default for MozAdsClientBuilder {
    fn default() -> Self {
        Self(Mutex::new(MozAdsClientBuilderInner::default()))
    }
}

#[uniffi::export]
impl MozAdsClientBuilder {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(&self) -> MozAdsClient {
        let inner = self.0.lock();
        let client_config = AdsClientConfig {
            cache_config: inner.cache_config.clone().map(Into::into),
            environment: inner.environment.unwrap_or_default().into(),
            rotation_days: inner.rotation_days,
            telemetry: inner
                .telemetry
                .clone()
                .map(MozAdsTelemetryWrapper::new)
                .unwrap_or_else(MozAdsTelemetryWrapper::noop),
        };
        let client = AdsClient::new(client_config);
        MozAdsClient {
            inner: Mutex::new(client),
        }
    }

    pub fn cache_config(self: Arc<Self>, cache_config: MozAdsCacheConfig) -> Arc<Self> {
        self.0.lock().cache_config = Some(cache_config);
        self
    }

    pub fn environment(self: Arc<Self>, environment: MozAdsEnvironment) -> Arc<Self> {
        self.0.lock().environment = Some(environment);
        self
    }

    pub fn rotation_days(self: Arc<Self>, rotation_days: u8) -> Arc<Self> {
        self.0.lock().rotation_days = Some(rotation_days);
        self
    }

    pub fn telemetry(self: Arc<Self>, telemetry: Arc<dyn MozAdsTelemetry>) -> Arc<Self> {
        self.0.lock().telemetry = Some(telemetry);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum, Eq, PartialEq)]
pub enum MozAdsEnvironment {
    #[default]
    Prod,
    Staging,
}

#[derive(Clone, uniffi::Record)]
pub struct MozAdsCacheConfig {
    pub db_path: String,
    #[uniffi(default = None)]
    pub default_cache_ttl_seconds: Option<u64>,
    #[uniffi(default = None)]
    pub max_size_mib: Option<u64>,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsContentCategory {
    pub categories: Vec<String>,
    pub taxonomy: MozAdsIABContentTaxonomy,
}

#[derive(Clone, Copy, Debug, uniffi::Enum, PartialEq)]
pub enum MozAdsIABContentTaxonomy {
    IAB1_0,
    IAB2_0,
    IAB2_1,
    IAB2_2,
    IAB3_0,
}

#[derive(Clone, Copy, Debug, Default, uniffi::Record)]
pub struct MozAdsCachePolicy {
    pub mode: MozAdsCacheMode,
    #[uniffi(default = None)]
    pub ttl_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum MozAdsReportReason {
    Inappropriate,
    NotInterested,
    SeenTooManyTimes,
}

impl From<MozAdsReportReason> for ReportReason {
    fn from(reason: MozAdsReportReason) -> Self {
        match reason {
            MozAdsReportReason::Inappropriate => ReportReason::Inappropriate,
            MozAdsReportReason::NotInterested => ReportReason::NotInterested,
            MozAdsReportReason::SeenTooManyTimes => ReportReason::SeenTooManyTimes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, uniffi::Enum)]
pub enum MozAdsCacheMode {
    #[default]
    CacheFirst,
    NetworkFirst,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsImage {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: MozAdsCallbacks,
    pub format: String,
    pub image_url: String,
    pub url: String,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsSpoc {
    pub block_key: String,
    pub callbacks: MozAdsCallbacks,
    pub caps: MozAdsSpocFrequencyCaps,
    pub domain: String,
    pub excerpt: String,
    pub format: String,
    pub image_url: String,
    pub ranking: MozAdsSpocRanking,
    pub sponsor: String,
    pub sponsored_by_override: Option<String>,
    pub title: String,
    pub url: String,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsSpocFrequencyCaps {
    pub cap_key: String,
    pub day: u32,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsSpocRanking {
    pub item_score: f64,
    pub personalization_models: std::collections::HashMap<String, u32>,
    pub priority: u32,
}

#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MozAdsTile {
    pub block_key: String,
    pub callbacks: MozAdsCallbacks,
    pub format: String,
    pub image_url: String,
    pub name: String,
    pub url: String,
}

impl From<AdCallbacks> for MozAdsCallbacks {
    fn from(callbacks: AdCallbacks) -> Self {
        Self {
            click: callbacks.click,
            impression: callbacks.impression,
            report: callbacks.report,
        }
    }
}

impl From<MozAdsCallbacks> for AdCallbacks {
    fn from(callbacks: MozAdsCallbacks) -> Self {
        Self {
            click: callbacks.click,
            impression: callbacks.impression,
            report: callbacks.report,
        }
    }
}

impl From<SpocFrequencyCaps> for MozAdsSpocFrequencyCaps {
    fn from(caps: SpocFrequencyCaps) -> Self {
        Self {
            cap_key: caps.cap_key,
            day: caps.day,
        }
    }
}

impl From<SpocRanking> for MozAdsSpocRanking {
    fn from(ranking: SpocRanking) -> Self {
        Self {
            item_score: ranking.item_score,
            personalization_models: ranking.personalization_models.unwrap_or_default(),
            priority: ranking.priority,
        }
    }
}

impl From<AdImage> for MozAdsImage {
    fn from(img: AdImage) -> Self {
        Self {
            alt_text: img.alt_text,
            block_key: img.block_key,
            callbacks: img.callbacks.into(),
            format: img.format,
            image_url: img.image_url,
            url: img.url,
        }
    }
}

impl From<AdSpoc> for MozAdsSpoc {
    fn from(spoc: AdSpoc) -> Self {
        Self {
            block_key: spoc.block_key,
            callbacks: spoc.callbacks.into(),
            caps: spoc.caps.into(),
            domain: spoc.domain,
            excerpt: spoc.excerpt,
            format: spoc.format,
            image_url: spoc.image_url,
            ranking: spoc.ranking.into(),
            sponsor: spoc.sponsor,
            sponsored_by_override: spoc.sponsored_by_override,
            title: spoc.title,
            url: spoc.url,
        }
    }
}

impl From<AdTile> for MozAdsTile {
    fn from(tile: AdTile) -> Self {
        Self {
            block_key: tile.block_key,
            callbacks: tile.callbacks.into(),
            format: tile.format,
            image_url: tile.image_url,
            name: tile.name,
            url: tile.url,
        }
    }
}

impl From<Environment> for MozAdsEnvironment {
    fn from(env: Environment) -> Self {
        match env {
            Environment::Prod => MozAdsEnvironment::Prod,
            Environment::Staging => MozAdsEnvironment::Staging,
            // `Environment::Test` only exists within `ads-client-core`'s own
            // `#[cfg(test)]` builds (used to point at a mockito server), so it is
            // never actually reachable here: this crate depends on
            // `ads-client-core` as an ordinary (non-test) dependency, even when
            // this crate's own tests are running.
        }
    }
}

impl From<MozAdsEnvironment> for Environment {
    fn from(env: MozAdsEnvironment) -> Self {
        match env {
            MozAdsEnvironment::Prod => Environment::Prod,
            MozAdsEnvironment::Staging => Environment::Staging,
        }
    }
}

impl From<IABContentTaxonomy> for MozAdsIABContentTaxonomy {
    fn from(taxonomy: IABContentTaxonomy) -> Self {
        match taxonomy {
            IABContentTaxonomy::IAB1_0 => MozAdsIABContentTaxonomy::IAB1_0,
            IABContentTaxonomy::IAB2_0 => MozAdsIABContentTaxonomy::IAB2_0,
            IABContentTaxonomy::IAB2_1 => MozAdsIABContentTaxonomy::IAB2_1,
            IABContentTaxonomy::IAB2_2 => MozAdsIABContentTaxonomy::IAB2_2,
            IABContentTaxonomy::IAB3_0 => MozAdsIABContentTaxonomy::IAB3_0,
        }
    }
}

impl From<MozAdsIABContentTaxonomy> for IABContentTaxonomy {
    fn from(taxonomy: MozAdsIABContentTaxonomy) -> Self {
        match taxonomy {
            MozAdsIABContentTaxonomy::IAB1_0 => IABContentTaxonomy::IAB1_0,
            MozAdsIABContentTaxonomy::IAB2_0 => IABContentTaxonomy::IAB2_0,
            MozAdsIABContentTaxonomy::IAB2_1 => IABContentTaxonomy::IAB2_1,
            MozAdsIABContentTaxonomy::IAB2_2 => IABContentTaxonomy::IAB2_2,
            MozAdsIABContentTaxonomy::IAB3_0 => IABContentTaxonomy::IAB3_0,
        }
    }
}

impl From<MozAdsCachePolicy> for CachePolicy {
    fn from(policy: MozAdsCachePolicy) -> Self {
        let ttl = policy.ttl_seconds.map(std::time::Duration::from_secs);
        match policy.mode {
            MozAdsCacheMode::CacheFirst => CachePolicy::CacheFirst { ttl },
            MozAdsCacheMode::NetworkFirst => CachePolicy::NetworkFirst { ttl },
        }
    }
}

impl From<&MozAdsIABContent> for AdContentCategory {
    fn from(content: &MozAdsIABContent) -> Self {
        Self {
            categories: content.category_ids.clone(),
            taxonomy: content.taxonomy.into(),
        }
    }
}

impl From<MozAdsRequestOptions> for CachePolicy {
    fn from(options: MozAdsRequestOptions) -> Self {
        options.cache_policy.map(Into::into).unwrap_or_default()
    }
}

// Note: `impl From<Option<MozAdsRequestOptions>> for CachePolicy` is not possible
// here: `Option` is not a "fundamental" type, so wrapping the (local)
// `MozAdsRequestOptions` in it doesn't satisfy Rust's orphan rules for a `From` impl
// on the foreign `CachePolicy` type. Callers should use
// `options.map(Into::into).unwrap_or_default()` directly instead.

impl From<MozAdsCacheConfig> for AdsCacheConfig {
    fn from(config: MozAdsCacheConfig) -> Self {
        Self {
            db_path: config.db_path,
            default_cache_ttl_seconds: config.default_cache_ttl_seconds,
            max_size_mib: config.max_size_mib,
        }
    }
}

impl From<&MozAdsPlacementRequest> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequest) -> Self {
        Self {
            content: request.iab_content.as_ref().map(Into::into),
            count: 1,
            placement: request.placement_id.clone(),
        }
    }
}

impl From<&MozAdsPlacementRequestWithCount> for AdPlacementRequest {
    fn from(request: &MozAdsPlacementRequestWithCount) -> Self {
        Self {
            content: request.iab_content.as_ref().map(Into::into),
            count: request.count,
            placement: request.placement_id.clone(),
        }
    }
}
