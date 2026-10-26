/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Sorts errors into expected ones, which a healthy device, network and server
//! can still produce, and unexpected ones, which point to a bug. Only fixed
//! kinds and codes are recorded, never an error's message.

use sql_support::open_database;
use viaduct::ViaductError;

use crate::client::error::{ComponentError, RequestAdsError};
use crate::http_cache::HttpCacheBuilderError;
use crate::mars::error::{
    CallbackRequestError, FetchAdsError, HTTPError, RecordClickError, RecordImpressionError,
    ReportAdError,
};

#[derive(Debug, PartialEq, Eq)]
pub struct ErrorKind {
    pub expected: bool,
    pub kind: &'static str,
    pub code: Option<String>,
    pub detail: Option<String>,
}

impl ErrorKind {
    pub fn expected(kind: &'static str) -> Self {
        Self {
            expected: true,
            kind,
            code: None,
            detail: None,
        }
    }

    pub fn unexpected(kind: &'static str) -> Self {
        Self {
            expected: false,
            ..Self::expected(kind)
        }
    }

    pub fn code(mut self, code: impl ToString) -> Self {
        self.code = Some(code.to_string());
        self
    }

    pub fn detail(mut self, detail: impl ToString) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
}

pub fn component(error: &ComponentError) -> ErrorKind {
    match error {
        ComponentError::RecordClick(RecordClickError::CallbackRequest(e))
        | ComponentError::RecordImpression(RecordImpressionError::CallbackRequest(e))
        | ComponentError::ReportAd(ReportAdError::CallbackRequest(e)) => callback(e),
        ComponentError::RequestAds(RequestAdsError::ContextId(_)) => {
            ErrorKind::unexpected("context_id")
        }
        ComponentError::RequestAds(RequestAdsError::FetchAds(e)) => fetch_ads(e),
    }
}

fn callback(error: &CallbackRequestError) -> ErrorKind {
    match error {
        // MARS no longer knows this ad.
        CallbackRequestError::HTTPError(HTTPError::Unexpected {
            code: code @ (404 | 410),
            ..
        }) => ErrorKind::expected("callback_gone").code(code),
        CallbackRequestError::HTTPError(e) => http(e),
        CallbackRequestError::InvalidUrl(_) => ErrorKind::unexpected("invalid_callback_url"),
        CallbackRequestError::Json(e) => invalid_response(e),
        CallbackRequestError::Request(e) => viaduct(e),
    }
}

fn fetch_ads(error: &FetchAdsError) -> ErrorKind {
    match error {
        FetchAdsError::BuildRequest(_) => ErrorKind::unexpected("invalid_request_config"),
        FetchAdsError::HTTPError(e) => http(e),
        FetchAdsError::Json(e) => invalid_response(e),
        FetchAdsError::Preflight(e) => callback(e),
        FetchAdsError::Request(e) => viaduct(e),
        FetchAdsError::Sqlite(e) => database(e),
        FetchAdsError::UrlParse(_) => ErrorKind::unexpected("invalid_url"),
    }
}

fn http(error: &HTTPError) -> ErrorKind {
    match error {
        HTTPError::BadRequest { code, .. } => ErrorKind::unexpected("http_bad_request").code(code),
        HTTPError::Server { code, .. } => ErrorKind::expected("http_server").code(code),
        HTTPError::Unexpected { code: 408, .. } => ErrorKind::expected("timeout").code(408),
        HTTPError::Unexpected { code: 429, .. } => ErrorKind::expected("http_throttled").code(429),
        HTTPError::Unexpected { code, .. } => {
            ErrorKind::unexpected("http_unexpected_status").code(code)
        }
    }
}

fn invalid_response(error: &serde_json::Error) -> ErrorKind {
    ErrorKind::unexpected("invalid_response").detail(serde_category(error))
}

/// A response item that did not deserialize, with the path of the field that
/// failed, e.g. `callbacks.click`.
pub fn invalid_ad_item(path: String) -> ErrorKind {
    ErrorKind::unexpected("invalid_ad_item").detail(path)
}

pub fn cache_builder(error: &HttpCacheBuilderError) -> ErrorKind {
    match error {
        HttpCacheBuilderError::Database(e) => database(e),
        HttpCacheBuilderError::EmptyDbPath
        | HttpCacheBuilderError::InvalidMaxSize { .. }
        | HttpCacheBuilderError::InvalidTtl { .. } => ErrorKind::unexpected("cache_config"),
    }
}

#[cfg(feature = "stateful")]
pub fn store_builder(error: &crate::ads_store::builder::AdsStoreBuilderError) -> ErrorKind {
    use crate::ads_store::builder::AdsStoreBuilderError;

    match error {
        AdsStoreBuilderError::Database(e) => database(e),
        AdsStoreBuilderError::EmptyDbPath | AdsStoreBuilderError::InvalidMaxSize { .. } => {
            ErrorKind::unexpected("cache_config")
        }
    }
}

pub fn database(error: &open_database::Error) -> ErrorKind {
    use rusqlite::ErrorCode;

    match error {
        open_database::Error::Shutdown => ErrorKind::expected("shutdown"),
        open_database::Error::IncompatibleVersion(_) => ErrorKind::unexpected("storage_downgrade"),
        open_database::Error::Corrupt | open_database::Error::RecoveryError(_) => {
            ErrorKind::unexpected("db_corrupt")
        }
        open_database::Error::SqlError(rusqlite::Error::SqliteFailure(failure, _)) => {
            let kind = match failure.code {
                ErrorCode::DiskFull | ErrorCode::SystemIoFailure => {
                    ErrorKind::expected("storage_full")
                }
                // We own every connection, with a busy timeout: contention is a bug.
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                    ErrorKind::unexpected("db_locked")
                }
                // By design: ErrorCode is non-exhaustive; anything else is a bug.
                _ => ErrorKind::unexpected("sql_error"),
            };
            kind.code(failure.extended_code)
        }
        open_database::Error::SqlError(_) => ErrorKind::unexpected("sql_error"),
    }
}

fn viaduct(error: &ViaductError) -> ErrorKind {
    match error {
        ViaductError::BackendError(message) => transport(message),
        ViaductError::OhttpRequestError(_) => ErrorKind::expected("ohttp_relay"),
        ViaductError::OhttpConfigFetchFailed(_) => ErrorKind::unexpected("ohttp_config"),
        ViaductError::OhttpResponseError(_) => ErrorKind::unexpected("ohttp_protocol"),
        ViaductError::UrlError(_) => ErrorKind::unexpected("invalid_url"),
        ViaductError::BackendAlreadyInitialized
        | ViaductError::BackendNotInitialized
        | ViaductError::NetworkError(_)
        | ViaductError::NonTlsUrl
        | ViaductError::OhttpChannelNotConfigured(_)
        | ViaductError::OhttpNotSupported
        | ViaductError::RequestHeaderError(_)
        | ViaductError::SetBackendError => ErrorKind::unexpected("misconfigured"),
    }
}

/// Reads the network error code out of a backend's error text, since no
/// backend reports a structured kind yet.
fn transport(message: &str) -> ErrorKind {
    // Desktop (necko) ends with "(0x804b000e)".
    let nsresult = message
        .trim_end()
        .strip_suffix(')')
        .and_then(|m| m.rsplit_once("(0x"))
        .and_then(|(_, hex)| u32::from_str_radix(hex, 16).ok());
    if let Some(code) = nsresult {
        let kind = match code {
            0x804B000E | 0x804B0055 => "timeout",
            0x804B0010 => "offline",
            0x804B001E => "dns",
            _ => "network_other",
        };
        return ErrorKind::expected(kind).code(format!("0x{code:08x}"));
    }
    // Android (GeckoView) mentions "error=0x33" or a SocketTimeoutException.
    let kind = if message.contains("error=0x33,") || message.contains("SocketTimeoutException") {
        "timeout"
    } else if message.contains("error=0x73,") {
        "offline"
    } else if message.contains("error=0x25,") {
        "dns"
    } else {
        // iOS (reqwest) does not report the cause.
        "network_other"
    };
    ErrorKind::expected(kind)
}

fn serde_category(error: &serde_json::Error) -> &'static str {
    match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(error: ComponentError) -> (bool, &'static str) {
        let kind = component(&error);
        (kind.expected, kind.kind)
    }

    fn fetch(error: FetchAdsError) -> ComponentError {
        ComponentError::RequestAds(RequestAdsError::FetchAds(error))
    }

    fn click(error: CallbackRequestError) -> ComponentError {
        ComponentError::RecordClick(RecordClickError::CallbackRequest(error))
    }

    fn status(code: u16) -> HTTPError {
        HTTPError::Unexpected {
            code,
            message: "body".into(),
        }
    }

    #[test]
    fn test_http_statuses() {
        let server = HTTPError::Server {
            code: 503,
            message: "body".into(),
        };
        assert_eq!(kind(fetch(server.into())), (true, "http_server"));
        assert_eq!(kind(fetch(status(429).into())), (true, "http_throttled"));
        assert_eq!(
            kind(fetch(status(404).into())),
            (false, "http_unexpected_status")
        );
        assert_eq!(kind(click(status(404).into())), (true, "callback_gone"));
        let bad = HTTPError::BadRequest {
            code: 400,
            message: "body".into(),
        };
        assert_eq!(kind(fetch(bad.into())), (false, "http_bad_request"));
    }

    #[test]
    fn test_transport_codes() {
        let backend = |m: &str| fetch(ViaductError::BackendError(m.into()).into());
        assert_eq!(
            kind(backend("Request failed (0x804b000e)")),
            (true, "timeout")
        );
        assert_eq!(kind(backend("Request failed (0x804b001e)")), (true, "dns"));
        assert_eq!(
            kind(backend(
                "IOException: WebRequestError: Request failed, error=0x33, category=0x3"
            )),
            (true, "timeout")
        );
        assert_eq!(
            kind(backend("error sending request for url (https://ads.test/)")),
            (true, "network_other")
        );
    }

    #[test]
    fn test_unexpected_errors() {
        let json = serde_json::from_str::<u32>("").unwrap_err();
        let classified = component(&fetch(json.into()));
        assert_eq!(
            (classified.expected, classified.kind),
            (false, "invalid_response")
        );
        assert_eq!(classified.detail.as_deref(), Some("eof"));

        let locked = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        let classified = database(&open_database::Error::SqlError(locked));
        assert_eq!((classified.expected, classified.kind), (false, "db_locked"));
        assert_eq!(classified.code.as_deref(), Some("5"));
    }
}
