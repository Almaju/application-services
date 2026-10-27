/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::telemetry;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use url::Url;

/// Response body of `getAds` (the `AdResponse` schema): the ads served for
/// each placement of the request.
///
/// An ad that does not match its schema is recorded and dropped, rather than
/// failing the whole response.
#[derive(Debug, Default, PartialEq)]
pub struct AdResponse {
    pub data: HashMap<String, Vec<Ad>>,
}

impl<'de> Deserialize<'de> for AdResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = HashMap::<String, serde_json::Value>::deserialize(deserializer)?;
        let data = raw
            .into_iter()
            .filter_map(|(placement, value)| match value {
                serde_json::Value::Array(items) => Some((
                    placement,
                    items
                        .into_iter()
                        .filter_map(|item| {
                            serde_json::from_value(item)
                                .inspect_err(telemetry::record_invalid_ad_item)
                                .ok()
                        })
                        .collect(),
                )),
                _ => None,
            })
            .collect();
        Ok(AdResponse { data })
    }
}

/// One ad of an [`AdResponse`]: `anyOf: [Image, Spoc, Tile]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Ad {
    Image(AdImage),
    Spoc(AdSpoc),
    Tile(AdTile),
}

// The schema has no discriminator, and `#[serde(untagged)]` would take the
// first variant that fits, but a spoc or a tile has every field an image has.
// `format` is what tells them apart: `spoc`, `tile`, or an image format such
// as `billboard`.
impl<'de> Deserialize<'de> for Ad {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let value = <serde_json::Value>::deserialize(deserializer)?;
        match value.get("format").and_then(serde_json::Value::as_str) {
            Some("spoc") => serde_json::from_value(value).map(Self::Spoc),
            Some("tile") => serde_json::from_value(value).map(Self::Tile),
            Some(_) => serde_json::from_value(value).map(Self::Image),
            None => return Err(D::Error::missing_field("format")),
        }
        .map_err(D::Error::custom)
    }
}

/// The `Image` schema.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdImage {
    pub alt_text: Option<String>,
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: Url,
    pub url: Url,
}

/// The `Spoc` schema.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdSpoc {
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub caps: SpocFrequencyCaps,
    pub domain: String,
    pub excerpt: String,
    pub format: String,
    pub image_url: Url,
    pub ranking: SpocRanking,
    pub sponsor: String,
    pub sponsored_by_override: Option<String>,
    pub title: String,
    pub url: Url,
}

/// The `Tile` schema.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdTile {
    pub block_key: String,
    pub callbacks: AdCallbacks,
    pub format: String,
    pub image_url: Url,
    pub name: String,
    pub url: Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpocFrequencyCaps {
    pub cap_key: String,
    pub day: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpocRanking {
    pub item_score: f64,
    pub personalization_models: Option<HashMap<String, u32>>,
    pub priority: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AdCallbacks {
    pub click: Url,
    pub impression: Url,
    pub report: Option<Url>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{
        get_example_happy_image_response, get_example_happy_spoc_response,
        get_example_happy_uatile_response,
    };
    use serde_json::{from_str, from_value, json};
    use url_macro::url;

    #[test]
    fn a_malformed_url_is_a_deserialization_error() {
        let response = json!({
            "block_key": "abc123",
            "callbacks": {
                "click": "https://buyanvilseveryday.test/click",
                "impression": "https://buyanvilseveryday.test/impression"
            },
            "format": "Leaderboard",
            "image_url": "not a url",
            "url": "https://buyanvilseveryday.test"
        })
        .to_string();

        // Before these fields were typed as `Url`, this parsed and handed
        // "not a url" straight to the caller.
        assert!(from_str::<AdImage>(&response).is_err());
    }

    #[test]
    fn test_moz_ad_full() {
        let response_full = json!({
            "alt_text": "An ad for an anvil",
            "block_key": "abc123",
            "callbacks": {
                "click": "https://buyanvilseveryday.test/click",
                "impression": "https://buyanvilseveryday.test/impression",
                "report": "https://buyanvilseveryday.test/report"
            },
            "format": "Leaderboard",
            "image_url": "https://buyanvilseveryday.test/img.png",
            "url": "https://buyanvilseveryday.test"
        })
        .to_string();

        let full: AdImage = from_str(&response_full).unwrap();
        assert_eq!(
            full,
            AdImage {
                alt_text: Some("An ad for an anvil".into()),
                block_key: "abc123".into(),
                callbacks: AdCallbacks {
                    click: Url::parse("https://buyanvilseveryday.test/click").unwrap(),
                    impression: Url::parse("https://buyanvilseveryday.test/impression").unwrap(),
                    report: Some(Url::parse("https://buyanvilseveryday.test/report").unwrap()),
                },
                format: "Leaderboard".into(),
                image_url: url!("https://buyanvilseveryday.test/img.png"),
                url: url!("https://buyanvilseveryday.test"),
            }
        );
    }

    #[test]
    fn test_moz_ad_response_partial() {
        let response_partial = json!({
            "alt_text": null,
            "block_key": "abc123",
            "callbacks": {
                "click": "https://example.test/click",
                "impression": "https://example.test/impression",
                "report": null
            },
            "format": "Leaderboard",
            "image_url": "https://example.test/image.png",
            "url": "https://example.test/item"
        })
        .to_string();

        let partial: AdImage = from_str(&response_partial).unwrap();
        assert_eq!(
            partial,
            AdImage {
                alt_text: None,
                block_key: "abc123".into(),
                callbacks: AdCallbacks {
                    click: Url::parse("https://example.test/click").unwrap(),
                    impression: Url::parse("https://example.test/impression").unwrap(),
                    report: None,
                },
                format: "Leaderboard".into(),
                image_url: url!("https://example.test/image.png"),
                url: url!("https://example.test/item"),
            }
        );
    }

    #[test]
    fn test_ad_response_serialization() {
        let raw_ad_response = json!({
            "missing_click_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "impression": "https://ads.fakeexample.org/impression/example_ad_1",
                    }
                }
            ],
            "incorrect_click_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "click": "incorrect-click-url",
                        "impression": "https://ads.fakeexample.org/impression/example_ad_1",
                    }
                }
            ],
            "missing_impression_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_1",
                    "image_url": "https://ads.fakeexample.org/example_image_1",
                    "format": "billboard",
                    "alt_text": "An ad for a puppy",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_1",
                    }
                }
            ],
            "incorrect_impression_url": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_2",
                    "image_url": "https://ads.fakeexample.org/example_image_2",
                    "format": "skyscraper",
                    "alt_text": "An ad for a pet duck",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_2",
                        "impression": "incorrect-impression-url",
                    }
                }
            ],
            "valid_ad": [
                {
                    "block_key": "abc123",
                    "url": "https://ads.fakeexample.org/example_ad_3",
                    "image_url": "https://ads.fakeexample.org/example_image_3",
                    "format": "skyscraper",
                    "alt_text": "An ad for a pet duck",
                    "callbacks": {
                        "click": "https://ads.fakeexample.org/click/example_ad_3",
                        "impression": "https://ads.fakeexample.org/impression/example_ad_3",
                        "report": "https://ads.fakeexample.org/report/example_ad_3"
                    }
                }
            ]
        });

        let parsed: AdResponse = from_value(raw_ad_response).unwrap();

        // The invalid ads are dropped, but their placements stay.
        let expected = AdResponse {
            data: HashMap::from([
                ("incorrect_click_url".to_string(), vec![]),
                ("incorrect_impression_url".to_string(), vec![]),
                ("missing_click_url".to_string(), vec![]),
                ("missing_impression_url".to_string(), vec![]),
                (
                    "valid_ad".to_string(),
                    vec![Ad::Image(AdImage {
                        url: url!("https://ads.fakeexample.org/example_ad_3"),
                        image_url: url!("https://ads.fakeexample.org/example_image_3"),
                        format: "skyscraper".to_string(),
                        block_key: "abc123".into(),
                        alt_text: Some("An ad for a pet duck".to_string()),
                        callbacks: AdCallbacks {
                            click: Url::parse("https://ads.fakeexample.org/click/example_ad_3")
                                .unwrap(),
                            impression: Url::parse(
                                "https://ads.fakeexample.org/impression/example_ad_3",
                            )
                            .unwrap(),
                            report: Some(
                                Url::parse("https://ads.fakeexample.org/report/example_ad_3")
                                    .unwrap(),
                            ),
                        },
                    })],
                ),
            ]),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_empty_ad_response_serialization() {
        let raw_ad_response = json!({
            "example_placement_1": [],
            "example_placement_2": []
        });

        let parsed: AdResponse = from_value(raw_ad_response).unwrap();

        let expected = AdResponse {
            data: HashMap::from([
                ("example_placement_1".to_string(), vec![]),
                ("example_placement_2".to_string(), vec![]),
            ]),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn ads_are_told_apart_by_format() {
        let first = |response: AdResponse| {
            response
                .data
                .into_values()
                .next()
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
        };
        let image = first(get_example_happy_image_response());
        let spoc = first(get_example_happy_spoc_response());
        let tile = first(get_example_happy_uatile_response());

        let roundtrip = |ad: &Ad| from_str::<Ad>(&serde_json::to_string(ad).unwrap()).unwrap();
        assert!(matches!(roundtrip(&image), Ad::Image(_)));
        assert!(matches!(roundtrip(&spoc), Ad::Spoc(_)));
        assert!(matches!(roundtrip(&tile), Ad::Tile(_)));

        let Ad::Spoc(spoc) = spoc else {
            panic!("example spoc response should hold spocs")
        };

        // Changing the spocs 'format' field to an image should result in it parsing to a image.
        let mut renamed = spoc.clone();
        renamed.format = "billboard".to_string();
        assert!(matches!(roundtrip(&Ad::Spoc(renamed)), Ad::Image(_)));

        // Changing the spocs 'format' field to something unrecognized should result in it parsing to an image as well.
        // TODO: This is not ultimately desired behaviour- see `Deserialize` implementation for `Ad`
        let mut renamed = spoc;
        renamed.format = "nonsense_format".to_string();
        assert!(matches!(roundtrip(&Ad::Spoc(renamed)), Ad::Image(_)));
    }

    #[test]
    fn an_ad_without_format_is_a_deserialization_error() {
        let ad = json!({
            "block_key": "abc123",
            "callbacks": {
                "click": "https://example.test/click",
                "impression": "https://example.test/impression"
            },
            "image_url": "https://example.test/image.png",
            "url": "https://example.test/item"
        });

        assert!(from_value::<Ad>(ad).is_err());
    }
}
