/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use viaduct::{Client, ClientSettings, Request, Response};

use super::error::{HTTPError, TransportError};
use super::preflight::Preflight;
use super::Environment;

const OHTTP_CHANNEL_ID: &str = "ads-client";

/// Sends MARS requests and checks their status. An OHTTP request goes
/// through the relay and carries the preflight's headers, since the relay
/// strips the user's own.
pub(super) struct MARSTransport {
    preflight: Preflight,
}

impl MARSTransport {
    pub(super) fn new(environment: &Environment) -> Self {
        Self {
            preflight: Preflight::new(environment),
        }
    }

    pub(super) fn send(
        &self,
        request: impl Into<Request>,
        ohttp: bool,
    ) -> Result<Response, TransportError> {
        let mut request = request.into();
        let client = if ohttp {
            request
                .headers
                .extend(self.preflight.headers(Self::send_direct)?);
            Client::with_ohttp_channel(OHTTP_CHANNEL_ID, ClientSettings::default())?
        } else {
            Client::new(ClientSettings::default())
        };
        Self::send_with(&client, request)
    }

    fn send_direct(request: Request) -> Result<Response, TransportError> {
        Self::send_with(&Client::new(ClientSettings::default()), request)
    }

    fn send_with(client: &Client, request: Request) -> Result<Response, TransportError> {
        let response = client.send_sync(request)?;
        HTTPError::check(&response)?;
        Ok(response)
    }
}
