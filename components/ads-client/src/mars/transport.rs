/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::hash::Hash;

use serde::{de::DeserializeOwned, Serialize};
use sql_support::open_database;
use viaduct::{Client, ClientSettings, Request};

use super::error::{HTTPError, TransportError};
use crate::{
    cache_control::CacheControl,
    http_cache::{HttpCache, RequestHash},
    telemetry, CachePolicy,
};

const OHTTP_CHANNEL_ID: &str = "ads-client";

pub struct MARSTransport {
    http_cache: Option<HttpCache>,
}

impl MARSTransport {
    pub fn new(http_cache: Option<HttpCache>) -> Self {
        Self { http_cache }
    }

    pub fn clear_cache(&self) -> Result<(), open_database::Error> {
        if let Some(cache) = self.http_cache.as_ref() {
            cache.clear()?;
        }
        Ok(())
    }

    pub fn fire(&self, request: Request, ohttp: bool) -> Result<(), TransportError> {
        let client = Self::client_for(ohttp)?;
        let response = client.send_sync(request)?;
        HTTPError::check(&response)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn invalidate_cache_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<(), open_database::Error> {
        if let Some(cache) = self.http_cache.as_ref() {
            cache.invalidate_by_hash(request_hash)?;
        }
        Ok(())
    }

    /// Sends `request` and deserializes the response, through the HTTP cache
    /// with `policy` when there is one.
    pub fn send<T, R>(
        &self,
        request: R,
        policy: &CachePolicy,
        ohttp: bool,
    ) -> Result<T, TransportError>
    where
        T: Serialize + DeserializeOwned,
        R: Hash + Into<Request>,
    {
        let client = Self::client_for(ohttp)?;
        let Some(cache) = self.http_cache.as_ref() else {
            return Self::fetch(&client, request).map(|(value, _)| value);
        };
        let request_hash = RequestHash::new(&request);
        let (value, outcomes) =
            cache.fetch_with_policy(&request_hash, policy, || Self::fetch(&client, request))?;
        for outcome in &outcomes {
            telemetry::record_http_cache_outcome(outcome);
        }
        Ok(value)
    }

    pub fn shutdown_db(&self) {
        if let Some(cache) = self.http_cache.as_ref() {
            cache.shutdown_db();
        }
    }

    fn fetch<T: DeserializeOwned>(
        client: &Client,
        request: impl Into<Request>,
    ) -> Result<(T, CacheControl), TransportError> {
        let response = client.send_sync(request.into())?;
        HTTPError::check(&response)?;
        Ok((response.json()?, CacheControl::from(&response)))
    }

    fn client_for(ohttp: bool) -> Result<Client, viaduct::ViaductError> {
        if ohttp {
            Client::with_ohttp_channel(OHTTP_CHANNEL_ID, ClientSettings::default())
        } else {
            Ok(Client::new(ClientSettings::default()))
        }
    }
}
