/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::{Deserialize, Serialize};

use crate::mars::ad_response::{AdImage, AdSpoc, AdTile};

/// The ads held for one placement. A placement only ever serves one kind of ad.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Ads {
    Image(Vec<AdImage>),
    Spoc(Vec<AdSpoc>),
    Tile(Vec<AdTile>),
}
