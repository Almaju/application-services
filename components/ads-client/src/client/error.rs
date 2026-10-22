/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::mars::error::{BuildRequestError, CallbackRequestError, FetchAdsError};

#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("Error recording a click for a placement: {0}")]
    RecordClick(#[from] RecordClickError),

    #[error("Error recording an impressions for a placement: {0}")]
    RecordImpression(#[from] RecordImpressionError),

    #[error("Error reporting an ad: {0}")]
    ReportAd(#[from] ReportAdError),

    #[error("Error requesting ads: {0}")]
    RequestAds(#[from] RequestAdsError),
}

#[derive(Debug, thiserror::Error)]
pub enum RequestAdsError {
    #[error("Error building ad request: {0}")]
    BuildRequest(#[from] BuildRequestError),

    #[error(transparent)]
    ContextId(#[from] context_id::ApiError),

    #[error("Error requesting ads from MARS: {0}")]
    FetchAds(#[from] FetchAdsError),

    #[error("OHTTP preflight failed: {0}")]
    Preflight(#[source] CallbackRequestError),
}

#[derive(Debug, thiserror::Error)]
pub enum RecordClickError {
    #[error("Callback request to MARS failed: {0}")]
    CallbackRequest(#[from] CallbackRequestError),
}

#[derive(Debug, thiserror::Error)]
pub enum RecordImpressionError {
    #[error("Callback request to MARS failed: {0}")]
    CallbackRequest(#[from] CallbackRequestError),
}

#[derive(Debug, thiserror::Error)]
pub enum ReportAdError {
    #[error("Callback request to MARS failed: {0}")]
    CallbackRequest(#[from] CallbackRequestError),
}
