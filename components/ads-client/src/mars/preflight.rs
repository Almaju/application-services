/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::Arc;
use url::Url;
use viaduct::{Headers, Request, Response};

use super::{error::TransportError, Environment};
use crate::cache_control::CacheControl;
use crate::clock::{CacheClock, Clock};

/// How long the headers are kept when the preflight response has no
/// `max-age`, as for any other cached response.
const DEFAULT_TTL_SECONDS: i64 = 300;

/// The headers an OHTTP request carries in place of the ones the relay
/// strips, from the MARS `/v1/ads-preflight` endpoint. They are kept in
/// memory for as long as the preflight response's `Cache-Control` allows.
pub(super) struct Preflight {
    cached: Mutex<Option<CachedHeaders>>,
    clock: Arc<dyn Clock>,
    url: Url,
}

struct CachedHeaders {
    expires_at: i64,
    headers: Headers,
}

impl Preflight {
    pub(super) fn new(environment: &Environment) -> Self {
        Self::with_clock(environment, Arc::new(CacheClock))
    }

    fn with_clock(environment: &Environment, clock: Arc<dyn Clock>) -> Self {
        Self {
            cached: Mutex::new(None),
            clock,
            url: environment.clone().into_url("ads-preflight"),
        }
    }

    /// The preflight's headers, fetched with `send` unless they are still
    /// cached.
    pub(super) fn headers(
        &self,
        send: impl FnOnce(Request) -> Result<Response, TransportError>,
    ) -> Result<Headers, TransportError> {
        let now = self.clock.now_epoch_seconds();
        if let Some(cached) = self
            .cached
            .lock()
            .as_ref()
            .filter(|cached| cached.expires_at > now)
        {
            return Ok(cached.headers.clone());
        }

        let (cache_control, preflight) = send(Request::get(self.url.clone()))
            .and_then(|response| {
                let preflight: PreflightResponse = response.json()?;
                Ok((CacheControl::from(&response), preflight))
            })
            .map_err(|e| TransportError::Preflight(Box::new(e)))?;
        let headers = Headers::try_from(preflight)?;
        if cache_control.should_cache() {
            let ttl = cache_control
                .max_age
                .map_or(DEFAULT_TTL_SECONDS, |max_age| {
                    i64::try_from(max_age).unwrap_or(i64::MAX)
                });
            *self.cached.lock() = Some(CachedHeaders {
                expires_at: now.saturating_add(ttl),
                headers: headers.clone(),
            });
        }
        Ok(headers)
    }
}

/// Response from the MARS `/v1/ads-preflight` endpoint.
#[derive(Debug, Deserialize)]
pub struct PreflightResponse {
    #[serde(default)]
    pub geo_location: String,
    #[serde(default)]
    pub normalized_ua: String,
}

impl TryFrom<PreflightResponse> for Headers {
    type Error = viaduct::ViaductError;

    fn try_from(preflight: PreflightResponse) -> Result<Self, Self::Error> {
        let mut headers = Headers::new();
        headers.insert("X-Geo-Location", preflight.geo_location)?;
        if !preflight.normalized_ua.is_empty() {
            headers.insert("X-User-Agent", preflight.normalized_ua)?;
        }
        Ok(headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use crate::mars::error::HTTPError;
    use mockito::mock;
    use viaduct::{Client, ClientSettings};

    const PREFLIGHT_BODY: &str = r#"{"geo_location":"US-CA","normalized_ua":"Firefox/140.0"}"#;

    fn send_direct(request: Request) -> Result<Response, TransportError> {
        let response = Client::new(ClientSettings::default()).send_sync(request)?;
        HTTPError::check(&response)?;
        Ok(response)
    }

    fn make_preflight() -> Preflight {
        Preflight::with_clock(&Environment::Test, Arc::new(TestClock::new(0)))
    }

    #[test]
    fn headers_are_kept_until_max_age() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/ads-preflight")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "max-age=60")
            .with_body(PREFLIGHT_BODY)
            .expect(2)
            .create();
        let preflight = make_preflight();

        let headers = preflight.headers(send_direct).unwrap();
        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), Some("Firefox/140.0"));

        // Still within max-age: no second request.
        preflight.headers(send_direct).unwrap();

        // Past max-age: fetched again.
        preflight.clock.advance(61);
        preflight.headers(send_direct).unwrap();
        m.assert();
    }

    #[test]
    fn no_store_headers_are_not_kept() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/ads-preflight")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "no-store")
            .with_body(PREFLIGHT_BODY)
            .expect(2)
            .create();
        let preflight = make_preflight();

        preflight.headers(send_direct).unwrap();
        preflight.headers(send_direct).unwrap();
        m.assert();
    }

    #[test]
    fn a_failed_preflight_is_a_preflight_error() {
        viaduct_dev::init_backend_dev();
        let m = mock("GET", "/ads-preflight")
            .with_status(500)
            .with_body("unavailable")
            .create();

        let result = make_preflight().headers(send_direct);
        assert!(matches!(result, Err(TransportError::Preflight(_))));
        m.assert();
    }

    #[test]
    fn headers_carry_geo_location_and_normalized_ua() {
        let headers = Headers::try_from(PreflightResponse {
            geo_location: "US-CA".to_string(),
            normalized_ua: "Firefox/140.0".to_string(),
        })
        .unwrap();

        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), Some("Firefox/140.0"));
    }

    #[test]
    fn empty_normalized_ua_is_omitted() {
        let headers = Headers::try_from(PreflightResponse {
            geo_location: "US-CA".to_string(),
            normalized_ua: String::new(),
        })
        .unwrap();

        assert_eq!(headers.get("X-Geo-Location"), Some("US-CA"));
        assert_eq!(headers.get("X-User-Agent"), None);
    }

    #[test]
    fn non_ascii_geo_location_is_an_error_not_a_panic() {
        let result = Headers::try_from(PreflightResponse {
            geo_location: "Zürich".to_string(),
            normalized_ua: String::new(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn header_injection_in_normalized_ua_is_an_error_not_a_panic() {
        let result = Headers::try_from(PreflightResponse {
            geo_location: "US-CA".to_string(),
            normalized_ua: "Firefox/140.0\r\nX-Injected: yes".to_string(),
        });

        assert!(result.is_err());
    }
}
