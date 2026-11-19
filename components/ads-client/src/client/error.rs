/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::mars::error::{FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError};
use crate::telemetry::ErrorEvent;

#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[cfg(feature = "stateful")]
    #[error("Error requesting ads from worker: {0}")]
    BackgroundWorker(#[from] BackgroundWorkerError),

    #[error("Error clearing the cache: {0}")]
    ClearCache(#[from] sql_support::open_database::Error),

    #[error("Error recording a click for a placement: {0}")]
    RecordClick(#[from] RecordClickError),

    #[error("Error recording an impressions for a placement: {0}")]
    RecordImpression(#[from] RecordImpressionError),

    #[error("Error reporting an ad: {0}")]
    ReportAd(#[from] ReportAdError),

    #[error("Error requesting ads: {0}")]
    RequestAds(#[from] RequestAdsError),
}

impl From<&ComponentError> for ErrorEvent {
    fn from(error: &ComponentError) -> Self {
        match error {
            #[cfg(feature = "stateful")]
            ComponentError::BackgroundWorker(_) => Self::unexpected("worker_closed"),
            ComponentError::ClearCache(e) => e.into(),
            ComponentError::RecordClick(RecordClickError::CallbackRequest(e))
            | ComponentError::RecordImpression(RecordImpressionError::CallbackRequest(e))
            | ComponentError::ReportAd(ReportAdError::CallbackRequest(e)) => e.into(),
            ComponentError::RequestAds(e) => e.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestAdsError {
    #[error(transparent)]
    ContextId(#[from] context_id::ApiError),

    #[error("Error requesting ads from MARS: {0}")]
    FetchAds(#[from] FetchAdsError),
}

impl From<&RequestAdsError> for ErrorEvent {
    fn from(error: &RequestAdsError) -> Self {
        match error {
            RequestAdsError::ContextId(_) => Self::unexpected("context_id"),
            RequestAdsError::FetchAds(e) => e.into(),
        }
    }
}

#[cfg(feature = "stateful")]
#[derive(Debug, thiserror::Error)]
pub enum BackgroundWorkerError {
    #[error("Background worker is closed")]
    Closed,
}
