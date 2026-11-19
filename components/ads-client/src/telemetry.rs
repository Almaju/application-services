/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Metric recording for the ads client, straight into the Glean metrics that
//! `build.rs` generates from `metrics.yaml`.
//!
//! The generated bindings call glean-sym, which records with the application's
//! Glean on Android and iOS. With the `glean-fog` feature, they call
//! Firefox-on-Glean instead, for Desktop, where only the parent process has a
//! Glean to record into.
//!
//! Every error the client records is an [`ErrorEvent`], which each module
//! builds from its own error types. Recording is infallible and silent by
//! design: telemetry must never change the outcome of the operation it is
//! describing.

#[cfg(feature = "glean-fog")]
use glean::private::RateMetric;
#[cfg(not(feature = "glean-fog"))]
use glean_sym::metrics::RateMetric;
use sql_support::open_database;
use viaduct::ViaductError;

mod glean_metrics {
    include!(concat!(env!("OUT_DIR"), "/glean_metrics.rs"));
}

use glean_metrics::ads_client;

/// An error as the `expected_error` and `unexpected_error` events record it.
/// None of it comes from the user or the server.
///
/// Expected: could happen on a healthy device, with a working network and
/// every party behaving correctly. Unexpected: anything else.
#[derive(Debug, PartialEq, Eq)]
pub enum ErrorEvent {
    Expected {
        code: Option<String>,
        kind: &'static str,
    },
    Unexpected {
        code: Option<String>,
        detail: Option<String>,
        kind: &'static str,
    },
}

impl ErrorEvent {
    pub(crate) fn expected(kind: &'static str) -> Self {
        Self::Expected { code: None, kind }
    }

    /// Android's GeckoView mentions "error=0x33" or a SocketTimeoutException.
    /// iOS's reqwest does not report the cause.
    fn geckoview(message: &str) -> Self {
        let kind = if message.contains("error=0x33,") || message.contains("SocketTimeoutException")
        {
            "timeout"
        } else if message.contains("error=0x73,") {
            "offline"
        } else if message.contains("error=0x25,") {
            "dns"
        } else {
            "network_other"
        };
        Self::expected(kind)
    }

    /// A response from MARS that does not deserialize.
    pub(crate) fn invalid_response(error: &serde_json::Error) -> Self {
        Self::unexpected_with_detail("invalid_response", Self::serde_category(error))
    }

    /// Desktop's necko ends its messages with an nsresult, such as
    /// "(0x804b000e)".
    fn necko(message: &str) -> Option<Self> {
        let code = message
            .trim_end()
            .strip_suffix(')')
            .and_then(|m| m.rsplit_once("(0x"))
            .and_then(|(_, hex)| u32::from_str_radix(hex, 16).ok())?;
        let kind = match code {
            0x804B000E | 0x804B0055 => "timeout",
            0x804B0010 => "offline",
            0x804B001E => "dns",
            _ => "network_other",
        };
        Some(Self::expected(kind).code(format!("0x{code:08x}")))
    }

    pub(crate) fn unexpected(kind: &'static str) -> Self {
        Self::Unexpected {
            code: None,
            detail: None,
            kind,
        }
    }

    pub(crate) fn unexpected_with_detail(kind: &'static str, detail: impl ToString) -> Self {
        Self::Unexpected {
            code: None,
            detail: Some(detail.to_string()),
            kind,
        }
    }

    /// An ad in a MARS response that does not deserialize.
    pub(crate) fn unparsable_ad(error: &serde_json::Error) -> Self {
        Self::unexpected_with_detail("invalid_ad_item", Self::serde_category(error))
    }

    pub(crate) fn code(mut self, code: impl ToString) -> Self {
        let (Self::Expected { code: slot, .. } | Self::Unexpected { code: slot, .. }) = &mut self;
        *slot = Some(code.to_string());
        self
    }

    /// `operation` is the public method the error failed, if it failed one.
    pub(crate) fn record(self, operation: Option<Operation>) {
        let operation = operation.map(|o| o.as_str().to_string());
        match self {
            Self::Expected { code, kind } => {
                ads_client::expected_error.record(ads_client::ExpectedErrorExtra {
                    code,
                    kind: Some(kind.to_string()),
                    operation,
                })
            }
            Self::Unexpected { code, detail, kind } => {
                ads_client::unexpected_error.record(ads_client::UnexpectedErrorExtra {
                    code,
                    detail,
                    kind: Some(kind.to_string()),
                    operation,
                })
            }
        }
    }

    fn serde_category(error: &serde_json::Error) -> &'static str {
        match error.classify() {
            serde_json::error::Category::Data => "data",
            serde_json::error::Category::Eof => "eof",
            serde_json::error::Category::Io => "io",
            serde_json::error::Category::Syntax => "syntax",
        }
    }
}

impl From<&open_database::Error> for ErrorEvent {
    fn from(error: &open_database::Error) -> Self {
        use rusqlite::ErrorCode;

        match error {
            open_database::Error::Shutdown => Self::expected("shutdown"),
            open_database::Error::IncompatibleVersion(_) => Self::unexpected("storage_downgrade"),
            open_database::Error::Corrupt | open_database::Error::RecoveryError(_) => {
                Self::unexpected("db_corrupt")
            }
            open_database::Error::SqlError(rusqlite::Error::SqliteFailure(failure, _)) => {
                let event = match failure.code {
                    ErrorCode::DiskFull | ErrorCode::SystemIoFailure => {
                        Self::expected("storage_full")
                    }
                    // We own every connection, with a busy timeout: contention is a bug.
                    ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                        Self::unexpected("db_locked")
                    }
                    // `ErrorCode` is non-exhaustive, and anything else is a bug.
                    _ => Self::unexpected("sql_error"),
                };
                event.code(failure.extended_code)
            }
            open_database::Error::SqlError(_) => Self::unexpected("sql_error"),
        }
    }
}

impl From<&ViaductError> for ErrorEvent {
    fn from(error: &ViaductError) -> Self {
        match error {
            // No backend reports a structured network error yet.
            ViaductError::BackendError(message) => {
                Self::necko(message).unwrap_or_else(|| Self::geckoview(message))
            }
            ViaductError::OhttpRequestError(_) => Self::expected("ohttp_relay"),
            ViaductError::OhttpConfigFetchFailed(_) => Self::unexpected("ohttp_config"),
            ViaductError::OhttpResponseError(_) => Self::unexpected("ohttp_protocol"),
            ViaductError::UrlError(_) => Self::unexpected("invalid_url"),
            ViaductError::BackendAlreadyInitialized
            | ViaductError::BackendNotInitialized
            | ViaductError::NetworkError(_)
            | ViaductError::NonTlsUrl
            | ViaductError::OhttpChannelNotConfigured(_)
            | ViaductError::OhttpNotSupported
            | ViaductError::RequestHeaderError(_)
            | ViaductError::SetBackendError => Self::unexpected("misconfigured"),
        }
    }
}

/// A public method of the client, as the error events name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    ClearCache,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestImageAds,
    RequestSpocAds,
    RequestSpocOrImageAds,
    RequestTileAds,
}

impl Operation {
    /// Records the outcome of this method, and passes it on.
    pub(crate) fn record<T, E>(self, result: Result<T, E>) -> Result<T, E>
    where
        for<'e> &'e E: Into<ErrorEvent>,
    {
        if let Some(rate) = self.success_rate() {
            count(rate, result.is_ok());
        }
        if let Err(error) = &result {
            error.into().record(Some(self));
        }
        result
    }

    /// Runs this ad request, recording how long the caller waited and its
    /// outcome.
    pub(crate) fn time_ad_request<T, E>(
        self,
        ohttp: bool,
        request: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E>
    where
        for<'e> &'e E: Into<ErrorEvent>,
    {
        let duration = if ohttp {
            &ads_client::request_duration_ohttp
        } else {
            &ads_client::request_duration_direct
        };
        let timer = duration.start();
        let result = request();
        duration.stop_and_accumulate(timer);
        self.record(result)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ClearCache => "clear_cache",
            Self::RecordClick => "record_click",
            Self::RecordImpression => "record_impression",
            Self::ReportAd => "report_ad",
            Self::RequestImageAds => "request_image_ads",
            Self::RequestSpocAds => "request_spoc_ads",
            Self::RequestSpocOrImageAds => "request_spoc_or_image_ads",
            Self::RequestTileAds => "request_tile_ads",
        }
    }

    fn success_rate(self) -> Option<&'static RateMetric> {
        match self {
            Self::RecordClick => Some(&*ads_client::click_success_rate),
            Self::RecordImpression => Some(&*ads_client::impression_success_rate),
            Self::RequestImageAds
            | Self::RequestSpocAds
            | Self::RequestSpocOrImageAds
            | Self::RequestTileAds => Some(&*ads_client::request_success_rate),
            Self::ClearCache | Self::ReportAd => None,
        }
    }
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records whether a cache-first MARS request was answered from the HTTP cache.
pub fn record_cache_hit(hit: bool) {
    count(&ads_client::cache_hit_rate, hit);
}

fn count(rate: &RateMetric, success: bool) {
    rate.add_to_denominator(1);
    if success {
        rate.add_to_numerator(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_errors_are_read_from_the_message() {
        let backend = |m: &str| ErrorEvent::from(&ViaductError::BackendError(m.into()));
        assert_eq!(
            backend("Request failed (0x804b000e)"),
            ErrorEvent::expected("timeout").code("0x804b000e")
        );
        assert_eq!(
            backend("Request failed (0x804b001e)"),
            ErrorEvent::expected("dns").code("0x804b001e")
        );
        assert_eq!(
            backend("IOException: WebRequestError: Request failed, error=0x33, category=0x3"),
            ErrorEvent::expected("timeout")
        );
        assert_eq!(
            backend("error sending request for url (https://ads.test/)"),
            ErrorEvent::expected("network_other")
        );
    }

    #[test]
    fn test_database_errors_keep_the_sqlite_code() {
        let locked = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        assert_eq!(
            ErrorEvent::from(&open_database::Error::SqlError(locked)),
            ErrorEvent::unexpected("db_locked").code(5)
        );
    }

    #[test]
    fn test_invalid_json_records_only_its_category() {
        let error = serde_json::from_str::<u32>("").unwrap_err();
        assert_eq!(
            ErrorEvent::invalid_response(&error),
            ErrorEvent::unexpected_with_detail("invalid_response", "eof")
        );
    }

    #[cfg(feature = "glean-fog")]
    mod glean {
        use std::sync::OnceLock;

        use ::glean::TestGetValue;
        use parking_lot::{Mutex, MutexGuard};

        use super::*;

        /// Gives a test a fresh Glean, holding off the other tests that read
        /// it. Tests that only record may still run alongside.
        fn reset_glean() -> MutexGuard<'static, ()> {
            static DATA_DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
            static LOCK: Mutex<()> = Mutex::new(());

            let lock = LOCK.lock();
            let data_dir = DATA_DIR.get_or_init(|| tempfile::tempdir().unwrap());
            ::glean::test_reset_glean(
                ::glean::ConfigurationBuilder::new(true, data_dir.path(), "ads-client-test")
                    .build(),
                ::glean::ClientInfoMetrics::unknown(),
                true,
            );
            lock
        }

        #[test]
        fn test_ad_request_records_its_duration_and_success() {
            let _glean = reset_glean();

            Operation::RequestTileAds
                .time_ad_request(true, || Ok::<_, ViaductError>(()))
                .unwrap();

            let rate = ads_client::request_success_rate
                .test_get_value(None)
                .unwrap();
            assert!(rate.numerator >= 1 && rate.denominator >= rate.numerator);
            assert!(ads_client::request_duration_ohttp
                .test_get_value(None)
                .is_some_and(|duration| duration.count >= 1));
        }

        #[test]
        fn test_failed_operation_records_an_error_event() {
            let _glean = reset_glean();
            let error = ViaductError::BackendError("Request failed (0x804b0010)".into());

            assert!(Operation::RecordClick.record(Err::<(), _>(error)).is_err());

            let rate = ads_client::click_success_rate.test_get_value(None).unwrap();
            assert!(rate.denominator > rate.numerator);
            let events = ads_client::expected_error.test_get_value(None).unwrap();
            assert!(events.iter().any(|event| {
                event.extra.as_ref().is_some_and(|extra| {
                    extra.get("kind").map(String::as_str) == Some("offline")
                        && extra.get("operation").map(String::as_str) == Some("record_click")
                        && extra.get("code").map(String::as_str) == Some("0x804b0010")
                })
            }));
        }
    }
}
