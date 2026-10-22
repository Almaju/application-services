/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Decides, for every error the client can produce, whether it is expected
//! (tier 2) or unexpected (tier 3), and what we may record about it.
//!
//! The rule: could this happen on a healthy device, with a working network and
//! every party behaving correctly? Yes means expected, no means unexpected.
//!
//! Every match below is exhaustive, so a new error variant does not build until
//! it is classified. Nothing here records an error's `Display`: messages are
//! static, except serde's, which go through the scrubber.

use std::borrow::Cow;

use rusqlite::ErrorCode;
use sql_support::open_database;
use viaduct::ViaductError;

use super::scrub::scrub;
use crate::client::error::{ComponentError, RequestAdsError};
use crate::http_cache::HttpCacheBuilderError;
use crate::mars::error::{
    BuildRequestError, CallbackRequestError, FetchAdsError, HTTPError, RecordClickError,
    RecordImpressionError, ReportAdError, TransportError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Expected,
    Unexpected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    // Expected.
    CallbackGone,
    Cancelled,
    Connection,
    Dns,
    HttpServer,
    HttpThrottled,
    NetworkOther,
    Offline,
    OhttpConfig,
    OhttpRelay,
    Proxy,
    Shutdown,
    StorageBusy,
    StorageDowngrade,
    Timeout,
    Tls,
    // Unexpected.
    CacheConfig,
    ContextId,
    DbCorrupt,
    DbRecovery,
    HttpBadRequest,
    HttpUnexpectedStatus,
    InvalidAdItem,
    InvalidCallbackUrl,
    InvalidPlacement,
    InvalidPreflight,
    InvalidRequestConfig,
    InvalidResponse,
    InvalidUrl,
    Misconfigured,
    OhttpProtocol,
    SqlError,
}

impl ErrorKind {
    #[cfg(test)]
    pub const ALL: [ErrorKind; 32] = [
        Self::CallbackGone,
        Self::Cancelled,
        Self::Connection,
        Self::Dns,
        Self::HttpServer,
        Self::HttpThrottled,
        Self::NetworkOther,
        Self::Offline,
        Self::OhttpConfig,
        Self::OhttpRelay,
        Self::Proxy,
        Self::Shutdown,
        Self::StorageBusy,
        Self::StorageDowngrade,
        Self::Timeout,
        Self::Tls,
        Self::CacheConfig,
        Self::ContextId,
        Self::DbCorrupt,
        Self::DbRecovery,
        Self::HttpBadRequest,
        Self::HttpUnexpectedStatus,
        Self::InvalidAdItem,
        Self::InvalidCallbackUrl,
        Self::InvalidPlacement,
        Self::InvalidPreflight,
        Self::InvalidRequestConfig,
        Self::InvalidResponse,
        Self::InvalidUrl,
        Self::Misconfigured,
        Self::OhttpProtocol,
        Self::SqlError,
    ];

    pub fn tier(self) -> Tier {
        match self {
            Self::CallbackGone
            | Self::Cancelled
            | Self::Connection
            | Self::Dns
            | Self::HttpServer
            | Self::HttpThrottled
            | Self::NetworkOther
            | Self::Offline
            | Self::OhttpConfig
            | Self::OhttpRelay
            | Self::Proxy
            | Self::Shutdown
            | Self::StorageBusy
            | Self::StorageDowngrade
            | Self::Timeout
            | Self::Tls => Tier::Expected,
            Self::CacheConfig
            | Self::ContextId
            | Self::DbCorrupt
            | Self::DbRecovery
            | Self::HttpBadRequest
            | Self::HttpUnexpectedStatus
            | Self::InvalidAdItem
            | Self::InvalidCallbackUrl
            | Self::InvalidPlacement
            | Self::InvalidPreflight
            | Self::InvalidRequestConfig
            | Self::InvalidResponse
            | Self::InvalidUrl
            | Self::Misconfigured
            | Self::OhttpProtocol
            | Self::SqlError => Tier::Unexpected,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CacheConfig => "cache_config",
            Self::CallbackGone => "callback_gone",
            Self::Cancelled => "cancelled",
            Self::Connection => "connection",
            Self::ContextId => "context_id",
            Self::DbCorrupt => "db_corrupt",
            Self::DbRecovery => "db_recovery",
            Self::Dns => "dns",
            Self::HttpBadRequest => "http_bad_request",
            Self::HttpServer => "http_server",
            Self::HttpThrottled => "http_throttled",
            Self::HttpUnexpectedStatus => "http_unexpected_status",
            Self::InvalidAdItem => "invalid_ad_item",
            Self::InvalidCallbackUrl => "invalid_callback_url",
            Self::InvalidPlacement => "invalid_placement",
            Self::InvalidPreflight => "invalid_preflight",
            Self::InvalidRequestConfig => "invalid_request_config",
            Self::InvalidResponse => "invalid_response",
            Self::InvalidUrl => "invalid_url",
            Self::Misconfigured => "misconfigured",
            Self::NetworkOther => "network_other",
            Self::Offline => "offline",
            Self::OhttpConfig => "ohttp_config",
            Self::OhttpProtocol => "ohttp_protocol",
            Self::OhttpRelay => "ohttp_relay",
            Self::Proxy => "proxy",
            Self::Shutdown => "shutdown",
            Self::SqlError => "sql_error",
            Self::StorageBusy => "storage_busy",
            Self::StorageDowngrade => "storage_downgrade",
            Self::Timeout => "timeout",
            Self::Tls => "tls",
        }
    }
}

/// What may be recorded about one error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classified {
    pub kind: ErrorKind,
    /// Variant names from the outermost error to the leaf.
    pub chain: Vec<&'static str>,
    pub message: Cow<'static, str>,
    /// The HTTP status, network error code or SQLite extended code.
    pub code: Option<String>,
    pub context: Vec<(&'static str, String)>,
}

impl Classified {
    pub fn new(
        kind: ErrorKind,
        variant: &'static str,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            kind,
            chain: vec![variant],
            message: message.into(),
            code: None,
            context: vec![],
        }
    }

    pub fn tier(&self) -> Tier {
        self.kind.tier()
    }

    pub fn leaf(&self) -> &'static str {
        self.chain.last().copied().unwrap_or_default()
    }

    pub fn code(mut self, code: impl ToString) -> Self {
        self.code = Some(code.to_string());
        self
    }

    pub fn context(mut self, key: &'static str, value: impl ToString) -> Self {
        self.context.push((key, value.to_string()));
        self
    }

    fn wrapped(mut self, variant: &'static str) -> Self {
        self.chain.insert(0, variant);
        self
    }

    fn kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }
}

pub trait Classify {
    fn classify(&self) -> Classified;
}

impl Classify for ComponentError {
    fn classify(&self) -> Classified {
        match self {
            Self::RecordClick(e) => e.classify().wrapped("ComponentError::RecordClick"),
            Self::RecordImpression(e) => e.classify().wrapped("ComponentError::RecordImpression"),
            Self::ReportAd(e) => e.classify().wrapped("ComponentError::ReportAd"),
            Self::RequestAds(e) => e.classify().wrapped("ComponentError::RequestAds"),
        }
    }
}

impl Classify for RequestAdsError {
    fn classify(&self) -> Classified {
        match self {
            Self::ContextId(_) => Classified::new(
                ErrorKind::ContextId,
                "RequestAdsError::ContextId",
                "context id unavailable",
            ),
            Self::FetchAds(e) => e.classify().wrapped("RequestAdsError::FetchAds"),
        }
    }
}

impl Classify for RecordClickError {
    fn classify(&self) -> Classified {
        match self {
            Self::CallbackRequest(e) => e.classify().wrapped("RecordClickError::CallbackRequest"),
        }
    }
}

impl Classify for RecordImpressionError {
    fn classify(&self) -> Classified {
        match self {
            Self::CallbackRequest(e) => e
                .classify()
                .wrapped("RecordImpressionError::CallbackRequest"),
        }
    }
}

impl Classify for ReportAdError {
    fn classify(&self) -> Classified {
        match self {
            Self::CallbackRequest(e) => e.classify().wrapped("ReportAdError::CallbackRequest"),
        }
    }
}

impl Classify for CallbackRequestError {
    fn classify(&self) -> Classified {
        match self {
            Self::HTTPError(e) => {
                let classified = e.classify().wrapped("CallbackRequestError::HTTPError");
                // A callback URL that no longer exists is MARS's call to make.
                match e {
                    HTTPError::Unexpected {
                        code: 404 | 410, ..
                    } => classified.kind(ErrorKind::CallbackGone),
                    HTTPError::BadRequest { .. }
                    | HTTPError::Server { .. }
                    | HTTPError::Unexpected { .. } => classified,
                }
            }
            Self::InvalidUrl(e) => Classified::new(
                ErrorKind::InvalidCallbackUrl,
                "CallbackRequestError::InvalidUrl",
                // url's messages are static and never contain the input.
                e.to_string(),
            ),
            Self::Json(e) => {
                classify_serde(ErrorKind::InvalidPreflight, "CallbackRequestError::Json", e)
            }
            Self::Request(e) => e.classify().wrapped("CallbackRequestError::Request"),
        }
    }
}

impl Classify for FetchAdsError {
    fn classify(&self) -> Classified {
        match self {
            Self::BuildRequest(e) => e.classify().wrapped("FetchAdsError::BuildRequest"),
            Self::HTTPError(e) => e.classify().wrapped("FetchAdsError::HTTPError"),
            Self::Json(e) => classify_serde(ErrorKind::InvalidResponse, "FetchAdsError::Json", e),
            Self::Preflight(e) => e.classify().wrapped("FetchAdsError::Preflight"),
            Self::Request(e) => e.classify().wrapped("FetchAdsError::Request"),
            Self::Sqlite(e) => e.classify().wrapped("FetchAdsError::Sqlite"),
            Self::UrlParse(e) => Classified::new(
                ErrorKind::InvalidUrl,
                "FetchAdsError::UrlParse",
                e.to_string(),
            ),
        }
    }
}

impl Classify for TransportError {
    fn classify(&self) -> Classified {
        match self {
            Self::Http(e) => e.classify().wrapped("TransportError::Http"),
            Self::Json(e) => classify_serde(ErrorKind::InvalidResponse, "TransportError::Json", e),
            Self::Request(e) => e.classify().wrapped("TransportError::Request"),
        }
    }
}

impl Classify for BuildRequestError {
    fn classify(&self) -> Classified {
        // Both variants are a caller bug. The placement id stays out.
        let variant = match self {
            Self::DuplicatePlacementId { .. } => "BuildRequestError::DuplicatePlacementId",
            Self::EmptyConfig => "BuildRequestError::EmptyConfig",
        };
        Classified::new(
            ErrorKind::InvalidRequestConfig,
            variant,
            "invalid placement requests",
        )
    }
}

impl Classify for HTTPError {
    fn classify(&self) -> Classified {
        // The body is arbitrary server text: only its size is recorded.
        let (kind, variant, code, body) = match self {
            Self::BadRequest { code, message } => (
                ErrorKind::HttpBadRequest,
                "HTTPError::BadRequest",
                *code,
                message,
            ),
            Self::Server { code, message } => {
                (ErrorKind::HttpServer, "HTTPError::Server", *code, message)
            }
            Self::Unexpected { code, message } => {
                let kind = match code {
                    408 => ErrorKind::Timeout,
                    429 => ErrorKind::HttpThrottled,
                    _ => ErrorKind::HttpUnexpectedStatus,
                };
                (kind, "HTTPError::Unexpected", *code, message)
            }
        };
        Classified::new(kind, variant, "unsuccessful HTTP status")
            .code(code)
            .context("body_bytes", body.len())
    }
}

impl Classify for HttpCacheBuilderError {
    fn classify(&self) -> Classified {
        match self {
            Self::Database(e) => e.classify().wrapped("HttpCacheBuilderError::Database"),
            Self::EmptyDbPath => Classified::new(
                ErrorKind::CacheConfig,
                "HttpCacheBuilderError::EmptyDbPath",
                "empty cache path",
            ),
            Self::InvalidMaxSize { size_bytes, .. } => Classified::new(
                ErrorKind::CacheConfig,
                "HttpCacheBuilderError::InvalidMaxSize",
                "cache size out of range",
            )
            .context("size_bytes", size_bytes),
            Self::InvalidTtl { ttl, .. } => Classified::new(
                ErrorKind::CacheConfig,
                "HttpCacheBuilderError::InvalidTtl",
                "cache ttl out of range",
            )
            .context("ttl_s", ttl),
        }
    }
}

#[cfg(feature = "stateful")]
impl Classify for crate::ads_store::builder::AdsStoreBuilderError {
    fn classify(&self) -> Classified {
        match self {
            Self::Database(e) => e.classify().wrapped("AdsStoreBuilderError::Database"),
            Self::EmptyDbPath => Classified::new(
                ErrorKind::CacheConfig,
                "AdsStoreBuilderError::EmptyDbPath",
                "empty store path",
            ),
            Self::InvalidMaxSize { size_bytes, .. } => Classified::new(
                ErrorKind::CacheConfig,
                "AdsStoreBuilderError::InvalidMaxSize",
                "store size out of range",
            )
            .context("size_bytes", size_bytes),
        }
    }
}

impl Classify for open_database::Error {
    fn classify(&self) -> Classified {
        match self {
            Self::IncompatibleVersion(version) => Classified::new(
                ErrorKind::StorageDowngrade,
                "open_database::Error::IncompatibleVersion",
                "database from a newer version",
            )
            .context("db_version", version),
            Self::Corrupt => Classified::new(
                ErrorKind::DbCorrupt,
                "open_database::Error::Corrupt",
                "corrupt database",
            ),
            Self::SqlError(e) => classify_sqlite(e).wrapped("open_database::Error::SqlError"),
            Self::RecoveryError(e) => Classified::new(
                ErrorKind::DbRecovery,
                "open_database::Error::RecoveryError",
                "could not delete a corrupt database",
            )
            .context("io_kind", format!("{:?}", e.kind())),
            Self::Shutdown => Classified::new(
                ErrorKind::Shutdown,
                "open_database::Error::Shutdown",
                "database closed for shutdown",
            ),
        }
    }
}

impl Classify for ViaductError {
    fn classify(&self) -> Classified {
        let misconfigured =
            |variant| Classified::new(ErrorKind::Misconfigured, variant, "network setup error");
        match self {
            Self::BackendError(message) => {
                let (kind, code) = transport_kind(message);
                let classified =
                    Classified::new(kind, "ViaductError::BackendError", "transport error");
                match code {
                    Some(code) => classified.code(code),
                    None => classified,
                }
            }
            Self::OhttpConfigFetchFailed(_) => Classified::new(
                ErrorKind::OhttpConfig,
                "ViaductError::OhttpConfigFetchFailed",
                "could not fetch the OHTTP config",
            ),
            // Mixes relay failures (expected) with encapsulation failures
            // until viaduct tells them apart.
            Self::OhttpRequestError(_) => Classified::new(
                ErrorKind::OhttpRelay,
                "ViaductError::OhttpRequestError",
                "OHTTP request failed",
            ),
            Self::OhttpResponseError(_) => Classified::new(
                ErrorKind::OhttpProtocol,
                "ViaductError::OhttpResponseError",
                "could not decapsulate the OHTTP response",
            ),
            Self::UrlError(_) => Classified::new(
                ErrorKind::InvalidUrl,
                "ViaductError::UrlError",
                "invalid URL",
            ),
            Self::BackendAlreadyInitialized => {
                misconfigured("ViaductError::BackendAlreadyInitialized")
            }
            Self::BackendNotInitialized => misconfigured("ViaductError::BackendNotInitialized"),
            // Only produced for an empty OHTTP gateway host.
            Self::NetworkError(_) => misconfigured("ViaductError::NetworkError"),
            Self::NonTlsUrl => misconfigured("ViaductError::NonTlsUrl"),
            Self::OhttpChannelNotConfigured(_) => {
                misconfigured("ViaductError::OhttpChannelNotConfigured")
            }
            Self::OhttpNotSupported => misconfigured("ViaductError::OhttpNotSupported"),
            Self::RequestHeaderError(_) => misconfigured("ViaductError::RequestHeaderError"),
            Self::SetBackendError => misconfigured("ViaductError::SetBackendError"),
        }
    }
}

/// Ad items in one response that did not deserialize, recorded as one error.
pub struct InvalidAdItems<'a> {
    pub count: usize,
    pub first: &'a serde_json::Error,
    pub first_index: usize,
    pub first_path: Option<String>,
}

impl Classify for InvalidAdItems<'_> {
    fn classify(&self) -> Classified {
        let classified = classify_serde(
            ErrorKind::InvalidAdItem,
            "AdResponse::invalid_item",
            self.first,
        )
        .context("invalid_items", self.count)
        .context("item_index", self.first_index);
        match &self.first_path {
            Some(path) => classified.context("serde_path", path),
            None => classified,
        }
    }
}

/// A placement whose value is not an array of ads.
pub struct InvalidPlacement;

impl Classify for InvalidPlacement {
    fn classify(&self) -> Classified {
        Classified::new(
            ErrorKind::InvalidPlacement,
            "AdResponse::invalid_placement",
            "placement value is not an array",
        )
    }
}

fn classify_serde(kind: ErrorKind, variant: &'static str, error: &serde_json::Error) -> Classified {
    let class = match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    };
    Classified::new(kind, variant, scrub(&error.to_string())).context("serde_class", class)
}

fn classify_sqlite(error: &rusqlite::Error) -> Classified {
    let rusqlite::Error::SqliteFailure(failure, _) = error else {
        // By design: rusqlite::Error is non-exhaustive, and none of its other
        // variants are expected. Their messages can hold SQL or paths.
        return Classified::new(ErrorKind::SqlError, "rusqlite::Error", "SQLite error");
    };
    let kind = match failure.code {
        ErrorCode::DatabaseBusy
        | ErrorCode::DatabaseLocked
        | ErrorCode::DiskFull
        | ErrorCode::SystemIoFailure => ErrorKind::StorageBusy,
        // By design: ErrorCode is non-exhaustive. Anything else is a bug.
        _ => ErrorKind::SqlError,
    };
    Classified::new(kind, "rusqlite::Error::SqliteFailure", "SQLite error")
        .code(failure.extended_code)
}

/// Reads the network error code out of a backend's error text. Each backend
/// formats it differently, and none reports a structured kind yet.
pub fn transport_kind(message: &str) -> (ErrorKind, Option<String>) {
    // Desktop (necko): "<message> (0x804b000e)".
    if let Some(nsresult) = trailing_hex_in_parens(message) {
        let kind = match nsresult {
            0x804B000E | 0x804B0055 => ErrorKind::Timeout,
            0x804B0010 => ErrorKind::Offline,
            0x804B001E => ErrorKind::Dns,
            0x804B000D | 0x804B0014 | 0x804B0024 | 0x804B0047 | 0x804B004C => ErrorKind::Connection,
            0x804B002A | 0x804B0048 => ErrorKind::Proxy,
            0x80004004 => ErrorKind::Cancelled,
            code if code & 0xFFFF_0000 == 0x805A_0000 => ErrorKind::Tls,
            _ => ErrorKind::NetworkOther,
        };
        return (kind, Some(format!("0x{nsresult:08x}")));
    }
    // Android (GeckoView): "...WebRequestError: Request failed, error=0x33, category=0x3".
    if let Some(code) = hex_after(message, "error=0x") {
        let category = hex_after(message, "category=0x");
        let kind = match (code, category) {
            (0x33, _) => ErrorKind::Timeout,
            (0x73, _) => ErrorKind::Offline,
            (0x25, _) => ErrorKind::Dns,
            (0x23 | 0x43 | 0x93, _) => ErrorKind::Connection,
            (_, Some(0x2)) => ErrorKind::Tls,
            (_, Some(0x6)) => ErrorKind::Proxy,
            _ => ErrorKind::NetworkOther,
        };
        return (kind, Some(format!("gv:0x{code:x}")));
    }
    if message.contains("SocketTimeoutException") {
        return (ErrorKind::Timeout, Some("SocketTimeoutException".into()));
    }
    // iOS (reqwest) drops the cause from its message.
    (ErrorKind::NetworkOther, None)
}

fn trailing_hex_in_parens(message: &str) -> Option<u32> {
    let inner = message.trim_end().strip_suffix(')')?;
    let start = inner.rfind("(0x")?;
    u32::from_str_radix(&inner[start + 3..], 16).ok()
}

fn hex_after(message: &str, prefix: &str) -> Option<u32> {
    let start = message.find(prefix)? + prefix.len();
    let digits: String = message[start..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    u32::from_str_radix(&digits, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16) -> HTTPError {
        HTTPError::Unexpected {
            code: status,
            message: "body".into(),
        }
    }

    #[test]
    fn test_http_statuses() {
        let server = HTTPError::Server {
            code: 503,
            message: "x".repeat(10),
        }
        .classify();
        assert_eq!(server.kind, ErrorKind::HttpServer);
        assert_eq!(server.code.as_deref(), Some("503"));
        assert_eq!(server.context, vec![("body_bytes", "10".to_string())]);

        assert_eq!(http(429).classify().kind, ErrorKind::HttpThrottled);
        assert_eq!(http(408).classify().kind, ErrorKind::Timeout);
        assert_eq!(http(404).classify().kind, ErrorKind::HttpUnexpectedStatus);
        assert_eq!(
            CallbackRequestError::HTTPError(http(404)).classify().kind,
            ErrorKind::CallbackGone
        );
        assert_eq!(
            CallbackRequestError::HTTPError(http(401)).classify().kind,
            ErrorKind::HttpUnexpectedStatus
        );
    }

    #[test]
    fn test_chain_and_message_hide_the_body() {
        let error = ComponentError::RequestAds(RequestAdsError::FetchAds(
            FetchAdsError::HTTPError(HTTPError::BadRequest {
                code: 400,
                message: "context_id=secret".into(),
            }),
        ));
        let classified = error.classify();
        assert_eq!(classified.tier(), Tier::Unexpected);
        assert_eq!(
            classified.chain,
            vec![
                "ComponentError::RequestAds",
                "RequestAdsError::FetchAds",
                "FetchAdsError::HTTPError",
                "HTTPError::BadRequest",
            ]
        );
        assert!(!format!("{classified:?}").contains("secret"));
    }

    #[test]
    fn test_duplicate_placement_id_is_not_recorded() {
        let classified = BuildRequestError::DuplicatePlacementId {
            placement_id: "newtab_tile_1".into(),
        }
        .classify();
        assert_eq!(classified.kind, ErrorKind::InvalidRequestConfig);
        assert!(!format!("{classified:?}").contains("newtab_tile_1"));
    }

    #[test]
    fn test_transport_kinds() {
        assert_eq!(
            transport_kind("Request failed (0x804b000e)"),
            (ErrorKind::Timeout, Some("0x804b000e".into()))
        );
        assert_eq!(
            transport_kind("Request failed (0x804b001e)").0,
            ErrorKind::Dns
        );
        assert_eq!(
            transport_kind("Request failed (0x805a1ff3)").0,
            ErrorKind::Tls
        );
        assert_eq!(
            transport_kind(
                "UnexpectedUniFFICallbackError(reason: \"java.io.IOException: org.mozilla.geckoview.WebRequestError: Request failed, error=0x33, category=0x3\")"
            ),
            (ErrorKind::Timeout, Some("gv:0x33".into()))
        );
        assert_eq!(
            transport_kind(
                "UnexpectedUniFFICallbackError(reason: \"java.net.SocketTimeoutException\")"
            )
            .0,
            ErrorKind::Timeout
        );
        assert_eq!(
            transport_kind("error sending request for url (https://ads.mozilla.org/v1/ads)"),
            (ErrorKind::NetworkOther, None)
        );
    }

    #[test]
    fn test_viaduct_backend_error_records_no_text() {
        let classified =
            ViaductError::BackendError("error sending request for url (https://a.test/?q)".into())
                .classify();
        assert_eq!(classified.tier(), Tier::Expected);
        assert_eq!(classified.message, "transport error");
    }

    #[test]
    fn test_sqlite_codes() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("/Users/someone/profile/ads.db".into()),
        );
        let classified = open_database::Error::SqlError(busy).classify();
        assert_eq!(classified.kind, ErrorKind::StorageBusy);
        assert_eq!(classified.code.as_deref(), Some("5"));
        assert!(!format!("{classified:?}").contains("someone"));

        let readonly = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_READONLY),
            None,
        );
        assert_eq!(
            open_database::Error::SqlError(readonly).classify().kind,
            ErrorKind::SqlError
        );
    }
}
