/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use viaduct::{Client, ClientSettings, Request, Response};

use super::error::{HTTPError, TransportError};

const OHTTP_CHANNEL_ID: &str = "ads-client";

pub struct MARSTransport;

impl MARSTransport {
    pub fn fire(&self, request: Request, ohttp: bool) -> Result<(), TransportError> {
        let client = Self::client_for(ohttp)?;
        let response = client.send_sync(request)?;
        HTTPError::check(&response)?;
        Ok(())
    }

    pub fn send(
        &self,
        request: impl Into<Request>,
        ohttp: bool,
    ) -> Result<Response, TransportError> {
        let client = Self::client_for(ohttp)?;
        let response = client.send_sync(request.into())?;
        HTTPError::check(&response)?;
        Ok(response)
    }

    fn client_for(ohttp: bool) -> Result<Client, viaduct::ViaductError> {
        if ohttp {
            Client::with_ohttp_channel(OHTTP_CHANNEL_ID, ClientSettings::default())
        } else {
            Ok(Client::new(ClientSettings::default()))
        }
    }
}
