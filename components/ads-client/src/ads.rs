/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::http_cache::RequestHash;
use crate::mars::ad_response::{Ad, AdCallbacks, AdImage, AdResponse, AdSpoc, AdTile};
#[cfg(feature = "stateful")]
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// The ads held for one placement. A placement only ever serves one kind of ad.
#[cfg(feature = "stateful")]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Ads {
    Images(Vec<AdImage>),
    Spocs(Vec<AdSpoc>),
    Tiles(Vec<AdTile>),
}

#[cfg(feature = "stateful")]
impl Ads {
    /// Groups the ads MARS served for a placement under the kind of the first
    /// one, keeping only the ads of that kind. `None` if there are no ads.
    pub fn from_placement(ads: Vec<Ad>) -> Option<Self> {
        Some(match ads.first()? {
            Ad::Image(_) => Self::Images(ads.into_iter().filter_map(AdKind::from_ad).collect()),
            Ad::Spoc(_) => Self::Spocs(ads.into_iter().filter_map(AdKind::from_ad).collect()),
            Ad::Tile(_) => Self::Tiles(ads.into_iter().filter_map(AdKind::from_ad).collect()),
        })
    }
}

/// Identification of placement sent and returned from MARS (eg: `mock_spoc_1`)
#[cfg(feature = "stateful")]
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PlacementId(String);

#[cfg(feature = "stateful")]
impl PlacementId {
    pub fn new(s: &str) -> PlacementId {
        PlacementId(s.to_string())
    }
    pub fn into_inner(self) -> String {
        self.0
    }
}

#[cfg(feature = "stateful")]
impl AsRef<str> for PlacementId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "stateful")]
impl From<String> for PlacementId {
    fn from(value: String) -> Self {
        PlacementId(value)
    }
}

#[cfg(feature = "stateful")]
impl From<PlacementId> for String {
    fn from(value: PlacementId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AdImageOrSpoc {
    Image(AdImage),
    Spoc(AdSpoc),
}

/// A kind of ad callers ask for, picked out of the ads MARS served.
pub trait AdKind: Sized {
    /// `ad`, if it is of this kind.
    fn from_ad(ad: Ad) -> Option<Self>;
    fn callbacks_mut(&mut self) -> &mut AdCallbacks;
    fn cap_key(&self) -> Option<String> {
        None
    }
}

impl AdKind for AdImage {
    fn from_ad(ad: Ad) -> Option<Self> {
        match ad {
            Ad::Image(ad) => Some(ad),
            _ => None,
        }
    }

    fn callbacks_mut(&mut self) -> &mut AdCallbacks {
        &mut self.callbacks
    }
}

impl AdKind for AdSpoc {
    fn from_ad(ad: Ad) -> Option<Self> {
        match ad {
            Ad::Spoc(ad) => Some(ad),
            _ => None,
        }
    }

    fn callbacks_mut(&mut self) -> &mut AdCallbacks {
        &mut self.callbacks
    }

    fn cap_key(&self) -> Option<String> {
        Some(self.caps.cap_key.clone())
    }
}

impl AdKind for AdImageOrSpoc {
    fn from_ad(ad: Ad) -> Option<Self> {
        match ad {
            Ad::Image(ad) => Some(Self::Image(ad)),
            Ad::Spoc(ad) => Some(Self::Spoc(ad)),
            Ad::Tile(_) => None,
        }
    }

    fn callbacks_mut(&mut self) -> &mut AdCallbacks {
        match self {
            AdImageOrSpoc::Image(ad) => &mut ad.callbacks,
            AdImageOrSpoc::Spoc(ad) => &mut ad.callbacks,
        }
    }

    fn cap_key(&self) -> Option<String> {
        match self {
            AdImageOrSpoc::Image(_ad) => None,
            AdImageOrSpoc::Spoc(ad) => Some(ad.caps.cap_key.clone()),
        }
    }
}

impl AdKind for AdTile {
    fn from_ad(ad: Ad) -> Option<Self> {
        match ad {
            Ad::Tile(ad) => Some(ad),
            _ => None,
        }
    }

    fn callbacks_mut(&mut self) -> &mut AdCallbacks {
        &mut self.callbacks
    }
}

/// Keeps the ads of kind `A` that MARS served for each placement, dropping the
/// placements left without any.
///
/// Callbacks are tagged on the way: clicks and impressions with the request
/// hash (impressions also with the cap key of a spoc), reports with the ad's
/// placement and position.
pub fn select<A: AdKind>(
    response: AdResponse,
    request_hash: &RequestHash,
) -> HashMap<String, Vec<A>> {
    let request_hash = request_hash.to_string();
    response
        .data
        .into_iter()
        .filter_map(|(placement_id, ads)| {
            let mut ads: Vec<A> = ads.into_iter().filter_map(A::from_ad).collect();
            if ads.is_empty() {
                return None;
            }
            enrich_callbacks(&placement_id, &mut ads, &request_hash);
            Some((placement_id, ads))
        })
        .collect()
}

/// Keeps the first ad of each placement.
pub fn take_first<A>(placements: HashMap<String, Vec<A>>) -> HashMap<String, A> {
    placements
        .into_iter()
        .filter_map(|(placement_id, ads)| ads.into_iter().next().map(|ad| (placement_id, ad)))
        .collect()
}

fn enrich_callbacks<A: AdKind>(placement_id: &str, ads: &mut [A], request_hash: &str) {
    for (position, ad) in ads.iter_mut().enumerate() {
        let cap_key = ad.cap_key();
        let callbacks = ad.callbacks_mut();
        callbacks
            .click
            .query_pairs_mut()
            .append_pair("request_hash", request_hash);
        let mut impression_callback_query = callbacks.impression.query_pairs_mut();
        impression_callback_query.append_pair("request_hash", request_hash);
        if let Some(cap_key) = cap_key {
            impression_callback_query.append_pair("cap_key", &cap_key);
        }
        if let Some(report_url) = callbacks.report.as_mut() {
            report_url
                .query_pairs_mut()
                .append_pair("placement_id", placement_id)
                .append_pair("position", &position.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mars::ad_response::{SpocFrequencyCaps, SpocRanking};
    use crate::test_utils::{
        get_example_happy_image_response, get_example_happy_spoc_response,
        get_example_happy_uatile_response,
    };
    use url::Url;
    use url_macro::url;

    fn first_ad(response: AdResponse) -> Ad {
        response
            .data
            .into_values()
            .next()
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn select_keeps_the_requested_kind_and_drops_emptied_placements() {
        let image = first_ad(get_example_happy_image_response());
        let spoc = first_ad(get_example_happy_spoc_response());
        let tile = first_ad(get_example_happy_uatile_response());
        let response = || AdResponse {
            data: HashMap::from([
                ("images".to_string(), vec![image.clone()]),
                (
                    "mixed".to_string(),
                    vec![image.clone(), spoc.clone(), tile.clone()],
                ),
            ]),
        };
        let request_hash = RequestHash::from("abc123def456");

        let spocs: HashMap<String, Vec<AdSpoc>> = select(response(), &request_hash);
        assert_eq!(spocs.keys().collect::<Vec<_>>(), ["mixed"]);
        assert_eq!(spocs["mixed"].len(), 1);

        let tiles: HashMap<String, Vec<AdTile>> = select(response(), &request_hash);
        assert_eq!(tiles.keys().collect::<Vec<_>>(), ["mixed"]);
        assert_eq!(tiles["mixed"].len(), 1);

        let images: HashMap<String, Vec<AdImage>> = select(response(), &request_hash);
        assert_eq!(images["images"].len(), 1);
        assert_eq!(images["mixed"].len(), 1);

        let images_or_spocs: HashMap<String, Vec<AdImageOrSpoc>> =
            select(response(), &request_hash);
        assert_eq!(images_or_spocs["images"].len(), 1);
        assert!(matches!(
            images_or_spocs["mixed"].as_slice(),
            [AdImageOrSpoc::Image(_), AdImageOrSpoc::Spoc(_)]
        ));
    }

    #[cfg(feature = "stateful")]
    #[test]
    fn ads_take_their_kind_from_the_first_ad() {
        let image = first_ad(get_example_happy_image_response());
        let spoc = first_ad(get_example_happy_spoc_response());

        assert_eq!(Ads::from_placement(vec![]), None);
        assert!(matches!(
            Ads::from_placement(vec![spoc.clone(), image.clone()]),
            Some(Ads::Spocs(spocs)) if spocs.len() == 1
        ));
        assert!(matches!(
            Ads::from_placement(vec![image, spoc]),
            Some(Ads::Images(images)) if images.len() == 1
        ));
    }

    #[test]
    fn test_take_first() {
        let mut placements = HashMap::new();
        placements.insert(
            "placement_1".to_string(),
            vec![
                AdImage {
                    alt_text: Some("First ad".to_string()),
                    block_key: "key1".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click1").unwrap(),
                        impression: Url::parse("https://example.com/impression1").unwrap(),
                        report: None,
                    },
                    format: "billboard".to_string(),
                    image_url: url!("https://example.com/image1.png"),
                    url: url!("https://example.com/ad1"),
                },
                AdImage {
                    alt_text: Some("Second ad".to_string()),
                    block_key: "key2".to_string(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click2").unwrap(),
                        impression: Url::parse("https://example.com/impression2").unwrap(),
                        report: None,
                    },
                    format: "billboard".to_string(),
                    image_url: url!("https://example.com/image2.png"),
                    url: url!("https://example.com/ad2"),
                },
            ],
        );
        placements.insert(
            "placement_2".to_string(),
            vec![AdImage {
                alt_text: Some("Third ad".to_string()),
                block_key: "key3".to_string(),
                callbacks: AdCallbacks {
                    click: Url::parse("https://example.com/click3").unwrap(),
                    impression: Url::parse("https://example.com/impression3").unwrap(),
                    report: None,
                },
                format: "skyscraper".to_string(),
                image_url: url!("https://example.com/image3.png"),
                url: url!("https://example.com/ad3"),
            }],
        );
        placements.insert("placement_3".to_string(), vec![]);

        let result = take_first(placements);

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("placement_1"));
        assert!(result.contains_key("placement_2"));
        assert!(!result.contains_key("placement_3"));

        let first_ad = result.get("placement_1").unwrap();
        assert_eq!(first_ad.alt_text, Some("First ad".to_string()));
        assert_eq!(first_ad.block_key, "key1");

        let second_ad = result.get("placement_2").unwrap();
        assert_eq!(second_ad.alt_text, Some("Third ad".to_string()));
        assert_eq!(second_ad.block_key, "key3");
    }

    #[test]
    fn test_enrich_callbacks() {
        let response = AdResponse {
            data: HashMap::from([(
                "mock_tile_1".to_string(),
                vec![
                    Ad::Image(AdImage {
                        alt_text: None,
                        block_key: "key1".into(),
                        callbacks: AdCallbacks {
                            click: Url::parse("https://example.com/click1").unwrap(),
                            impression: Url::parse("https://example.com/impression1").unwrap(),
                            report: Some(Url::parse("https://example.com/report").unwrap()),
                        },
                        format: "billboard".to_string(),
                        image_url: url!("https://example.com/image1.png"),
                        url: url!("https://example.com/ad1"),
                    }),
                    Ad::Image(AdImage {
                        alt_text: None,
                        block_key: "key2".into(),
                        callbacks: AdCallbacks {
                            click: Url::parse("https://example.com/click2").unwrap(),
                            impression: Url::parse("https://example.com/impression2").unwrap(),
                            report: Some(Url::parse("https://example.com/report").unwrap()),
                        },
                        format: "billboard".to_string(),
                        image_url: url!("https://example.com/image2.png"),
                        url: url!("https://example.com/ad2"),
                    }),
                ],
            )]),
        };

        let request_hash = RequestHash::from("abc123def456");
        let placements: HashMap<String, Vec<AdImage>> = select(response, &request_hash);

        let ads = &placements["mock_tile_1"];

        // request_hash added to click and impression
        assert!(ads[0]
            .callbacks
            .click
            .query()
            .unwrap_or("")
            .contains("request_hash=abc123def456"));
        assert!(ads[0]
            .callbacks
            .impression
            .query()
            .unwrap_or("")
            .contains("request_hash=abc123def456"));

        // placement info added to report
        let report_0 = ads[0]
            .callbacks
            .report
            .as_ref()
            .unwrap()
            .query()
            .unwrap_or("");
        assert!(report_0.contains("placement_id=mock_tile_1"));
        assert!(report_0.contains("position=0"));

        let report_1 = ads[1]
            .callbacks
            .report
            .as_ref()
            .unwrap()
            .query()
            .unwrap_or("");
        assert!(report_1.contains("placement_id=mock_tile_1"));
        assert!(report_1.contains("position=1"));
    }

    #[test]
    fn test_enrich_callbacks_spoc_impressions_have_cap_key() {
        let response = AdResponse {
            data: HashMap::from([(
                "tile1".into(),
                vec![
                    Ad::Spoc(AdSpoc {
                        block_key: "block_key1".into(),
                        callbacks: AdCallbacks {
                            click: Url::parse("https://example.com/click1").unwrap(),
                            impression: Url::parse("https://example.com/impression1").unwrap(),
                            report: Some(Url::parse("https://example.com/report1").unwrap()),
                        },
                        caps: SpocFrequencyCaps {
                            cap_key: "cap_key1".into(),
                            day: 100,
                        },
                        domain: "1.example.com".into(),
                        excerpt: "excerpt1".into(),
                        format: "format1".into(),
                        image_url: url!("https://example.com/image1.png"),
                        url: url!("https://example.com/ad1"),
                        ranking: SpocRanking {
                            priority: 1,
                            personalization_models: None,
                            item_score: 1.0,
                        },
                        sponsor: "sponsor1".into(),
                        sponsored_by_override: None,
                        title: "title1".into(),
                    }),
                    Ad::Spoc(AdSpoc {
                        block_key: "block_key2".into(),
                        callbacks: AdCallbacks {
                            click: Url::parse("https://example.com/click2").unwrap(),
                            impression: Url::parse("https://example.com/impression2").unwrap(),
                            report: Some(Url::parse("https://example.com/report2").unwrap()),
                        },
                        caps: SpocFrequencyCaps {
                            cap_key: "cap_key2".into(),
                            day: 200,
                        },
                        domain: "2.example.com".into(),
                        excerpt: "excerpt2".into(),
                        format: "format2".into(),
                        image_url: url!("https://example.com/image2.png"),
                        url: url!("https://example.com/ad2"),
                        ranking: SpocRanking {
                            priority: 2,
                            personalization_models: None,
                            item_score: 2.0,
                        },
                        sponsor: "sponsor2".into(),
                        sponsored_by_override: None,
                        title: "title2".into(),
                    }),
                ],
            )]),
        };

        let request_hash = RequestHash::from("abc123def456");
        let placements: HashMap<String, Vec<AdSpoc>> = select(response, &request_hash);

        let ads = &placements["tile1"];

        assert!(ads[0]
            .callbacks
            .impression
            .query()
            .unwrap_or("")
            .contains("cap_key=cap_key1"));

        assert!(ads[1]
            .callbacks
            .impression
            .query()
            .unwrap_or("")
            .contains("cap_key=cap_key2"));
    }

    #[test]
    fn test_enrich_callbacks_skips_ads_without_report_url() {
        let response = AdResponse {
            data: HashMap::from([(
                "mock_tile_1".to_string(),
                vec![Ad::Image(AdImage {
                    alt_text: None,
                    block_key: "key1".into(),
                    callbacks: AdCallbacks {
                        click: Url::parse("https://example.com/click").unwrap(),
                        impression: Url::parse("https://example.com/impression").unwrap(),
                        report: None,
                    },
                    format: "billboard".to_string(),
                    image_url: url!("https://example.com/image.png"),
                    url: url!("https://example.com/ad"),
                })],
            )]),
        };

        // Should not panic
        let request_hash = RequestHash::from("abc123def456");
        let placements: HashMap<String, Vec<AdImage>> = select(response, &request_hash);

        let ad = &placements["mock_tile_1"][0];
        assert!(ad.callbacks.report.is_none());
        // click/impression still get request_hash
        assert!(ad
            .callbacks
            .click
            .query()
            .unwrap_or("")
            .contains("request_hash=abc123def456"));
        assert!(ad
            .callbacks
            .impression
            .query()
            .unwrap_or("")
            .contains("request_hash=abc123def456"));
    }
}
