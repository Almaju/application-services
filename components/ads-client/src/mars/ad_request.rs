/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use viaduct::{Headers, Request};

use crate::mars::Environment;

use super::error::BuildRequestError;

const ENDPOINT: &str = "/ads";

/// Request body of `getAds` (the `AdRequest` schema).
///
/// `flags` is not in the published schema yet, but MARS reads it.
#[derive(Debug, PartialEq, Serialize)]
pub struct AdRequest {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub flags: AdRequestFlags,
    pub placements: Vec<AdPlacementRequest>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub relevance: AdRequestRelevance,
}

impl AdRequest {
    pub fn try_new(
        blocks: Vec<String>,
        context_id: Option<String>,
        flags: AdRequestFlags,
        placements: Vec<AdPlacementRequest>,
        relevance: AdRequestRelevance,
    ) -> Result<Self, BuildRequestError> {
        if placements.is_empty() {
            return Err(BuildRequestError::EmptyConfig);
        };

        let mut used_placement_ids: HashSet<&str> = HashSet::new();
        for ad_placement_request in &placements {
            if !used_placement_ids.insert(&ad_placement_request.placement) {
                return Err(BuildRequestError::DuplicatePlacementId {
                    placement_id: ad_placement_request.placement.clone(),
                });
            }
        }

        Ok(AdRequest {
            blocks,
            context_id,
            flags,
            placements,
            relevance,
        })
    }
}

/// A `getAds` call as it goes over the wire: the body, plus where and how it
/// is sent. Its hash is the HTTP cache key.
pub struct GetAdsRequest<'a> {
    pub body: &'a AdRequest,
    pub environment: &'a Environment,
    pub headers: Headers,
    pub ohttp: bool,
}

/// Hash implementation intentionally excludes `context_id` as it rotates
/// on client re-instantiation and should not invalidate cached responses.
/// `headers` are also excluded as they are request metadata, not cache keys.
/// `flags` is hashed only when set so non-flag callers keep prior cache keys.
/// If response shape ever varies, add a version to this hash for variant tracking.
impl Hash for GetAdsRequest<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ENDPOINT.hash(state);
        self.environment.hash(state);
        if !self.body.flags.is_empty() {
            // HashMap is unordered — sort by key for a stable hash.
            let mut sorted: Vec<_> = self.body.flags.iter().collect();
            sorted.sort_unstable_by_key(|(k, _)| k.as_str());
            for (k, v) in sorted {
                k.hash(state);
                v.hash(state);
            }
        }
        if !self.body.relevance.is_empty() {
            // HashMap is unordered — sort by key for a stable hash.
            let mut sorted: Vec<_> = self.body.relevance.iter().collect();
            sorted.sort_unstable_by_key(|(k, _)| k.as_str());
            for (k, v) in sorted {
                k.hash(state);
                v.to_bits().hash(state);
            }
        }
        self.ohttp.hash(state);
        self.body.placements.hash(state);
    }
}

impl From<GetAdsRequest<'_>> for Request {
    fn from(get_ads: GetAdsRequest<'_>) -> Self {
        let url = get_ads.environment.clone().into_url(ENDPOINT);
        let mut request = Request::post(url).json(get_ads.body);
        request.headers.extend(get_ads.headers);
        request
    }
}

pub type AdRequestFlags = HashMap<String, bool>;
pub type AdRequestRelevance = HashMap<String, f64>;

/// The `AdPlacement` schema.
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct AdPlacementRequest {
    pub content: Option<AdContentCategory>,
    pub count: u32,
    pub placement: String,
}

/// The `AdContent` schema.
#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
pub struct AdContentCategory {
    pub categories: Vec<String>,
    pub taxonomy: IABContentTaxonomy,
}

#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
pub enum IABContentTaxonomy {
    #[serde(rename = "IAB-1.0")]
    IAB1_0,

    #[serde(rename = "IAB-2.0")]
    IAB2_0,

    #[serde(rename = "IAB-2.1")]
    IAB2_1,

    #[serde(rename = "IAB-2.2")]
    IAB2_2,

    #[serde(rename = "IAB-3.0")]
    IAB3_0,
}

#[cfg(test)]
mod tests {
    use crate::{http_cache::RequestHash, test_utils::TEST_CONTEXT_ID};

    use super::*;
    use serde_json::{json, to_value};
    use url::Url;

    fn hash_for(body: &AdRequest, environment: &Environment, ohttp: bool) -> RequestHash {
        RequestHash::new(&GetAdsRequest {
            body,
            environment,
            headers: Headers::new(),
            ohttp,
        })
    }

    fn hash(body: &AdRequest) -> RequestHash {
        hash_for(body, &Environment::Test, false)
    }

    #[test]
    fn test_ad_placement_request_with_content_serialize() {
        let request = AdPlacementRequest {
            content: Some(AdContentCategory {
                categories: vec!["Technology".into(), "Programming".into()],
                taxonomy: IABContentTaxonomy::IAB2_1,
            }),
            count: 1,
            placement: "example_placement".into(),
        };

        let serialized = to_value(&request).unwrap();

        let expected_json = json!({
            "placement": "example_placement",
            "count": 1,
            "content": {
                "taxonomy": "IAB-2.1",
                "categories": ["Technology", "Programming"]
            }
        });

        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_iab_content_taxonomy_serialize() {
        use serde_json::to_string;

        // We expect that enums map to strings like "IAB-2.2"
        let s = to_string(&IABContentTaxonomy::IAB1_0).unwrap();
        assert_eq!(s, "\"IAB-1.0\"");

        let s = to_string(&IABContentTaxonomy::IAB2_0).unwrap();
        assert_eq!(s, "\"IAB-2.0\"");

        let s = to_string(&IABContentTaxonomy::IAB2_1).unwrap();
        assert_eq!(s, "\"IAB-2.1\"");

        let s = to_string(&IABContentTaxonomy::IAB2_2).unwrap();
        assert_eq!(s, "\"IAB-2.2\"");

        let s = to_string(&IABContentTaxonomy::IAB3_0).unwrap();
        assert_eq!(s, "\"IAB-3.0\"");
    }

    #[test]
    fn test_build_ad_request_happy() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            HashMap::from([("contextual_placement".to_string(), true)]),
            vec![
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec!["entertainment".to_string()],
                        taxonomy: IABContentTaxonomy::IAB2_1,
                    }),
                    count: 1,
                    placement: "example_placement_1".to_string(),
                },
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec![],
                        taxonomy: IABContentTaxonomy::IAB2_1,
                    }),
                    count: 2,
                    placement: "example_placement_2".to_string(),
                },
            ],
            AdRequestRelevance::default(),
        )
        .unwrap();

        let expected_request = AdRequest {
            blocks: Default::default(),
            context_id: Some(TEST_CONTEXT_ID.to_string()),
            flags: HashMap::from([("contextual_placement".to_string(), true)]),
            placements: vec![
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec!["entertainment".to_string()],
                        taxonomy: IABContentTaxonomy::IAB2_1,
                    }),
                    count: 1,
                    placement: "example_placement_1".to_string(),
                },
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec![],
                        taxonomy: IABContentTaxonomy::IAB2_1,
                    }),
                    count: 2,
                    placement: "example_placement_2".to_string(),
                },
            ],
            relevance: AdRequestRelevance::default(),
        };

        assert_eq!(request, expected_request);
    }

    #[test]
    fn test_ad_request_omits_flags_when_none_are_set() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert!(request.flags.is_empty());

        let serialized = to_value(&request).unwrap();
        assert!(
            serialized.get("flags").is_none(),
            "flags object must be omitted from the wire when no flag is set, got: {serialized}"
        );
    }

    #[test]
    fn test_ad_request_serializes_explicit_false_flag() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            HashMap::from([("contextual_placement".to_string(), false)]),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        let serialized = to_value(&request).unwrap();
        assert_eq!(
            serialized.get("flags"),
            Some(&json!({"contextual_placement": false})),
            "Some(false) must round-trip onto the wire so callers can express explicit false",
        );
    }

    #[test]
    fn test_ad_request_omits_blocks_when_none() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert!(request.flags.is_empty());

        let serialized = to_value(&request).unwrap();
        assert!(
            serialized.get("blocks").is_none(),
            "blocks object must be omitted from the wire when no blocks are set, got: {serialized}"
        );
    }

    #[test]
    fn test_ad_request_includes_blocks_when_present() {
        let request = AdRequest::try_new(
            vec!["test".to_string()],
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert!(request.flags.is_empty());

        let serialized = to_value(&request).unwrap();
        assert!(
            serialized.get("blocks").is_some(),
            "blocks object must be included from the wire when blocks are set, got: {serialized}"
        );
    }

    #[test]
    fn test_ad_request_omits_relevance_when_empty() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        let serialized = to_value(&request).unwrap();
        assert!(
            serialized.get("relevance").is_none(),
            "relevance object must be omitted from the wire when empty, got: {serialized}"
        );
    }

    #[test]
    fn test_ad_request_includes_relevance_when_present() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            HashMap::from([
                ("sports".to_string(), 0.8),
                ("technology".to_string(), 0.25),
            ]),
        )
        .unwrap();

        let serialized = to_value(&request).unwrap();
        assert_eq!(
            serialized.get("relevance"),
            Some(&json!({"sports": 0.8, "technology": 0.25})),
        );
    }

    #[test]
    fn test_ad_request_omits_context_id_when_empty() {
        let request = AdRequest::try_new(
            Default::default(),
            None,
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "example_placement".to_string(),
            }],
            HashMap::from([("sports".to_string(), 0.8)]),
        )
        .unwrap();

        let serialized = to_value(&request).unwrap();
        assert!(
            serialized.get("context_id").is_none(),
            "context_id must be omitted from the wire when empty, got: {serialized}"
        );
    }

    #[test]
    fn test_ad_request_serializes_with_contextual_placement_flag_and_mixed_content() {
        let request = AdRequest::try_new(
            Default::default(),
            Some("context-123".to_string()),
            HashMap::from([("contextual_placement".to_string(), true)]),
            vec![
                AdPlacementRequest {
                    content: None,
                    count: 1,
                    placement: "newtab_stories_v2_1".to_string(),
                },
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec!["338".to_string()],
                        taxonomy: IABContentTaxonomy::IAB3_0,
                    }),
                    count: 1,
                    placement: "newtab_stories_v2_3".to_string(),
                },
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec!["596".to_string()],
                        taxonomy: IABContentTaxonomy::IAB3_0,
                    }),
                    count: 1,
                    placement: "newtab_stories_v2_4".to_string(),
                },
            ],
            AdRequestRelevance::default(),
        )
        .unwrap();

        let serialized = to_value(&request).unwrap();
        let expected_json = json!({
            "context_id": "context-123",
            "flags": {"contextual_placement": true},
            "placements": [
                {"placement": "newtab_stories_v2_1", "count": 1, "content": null},
                {"placement": "newtab_stories_v2_3", "count": 1, "content": {"taxonomy": "IAB-3.0", "categories": ["338"]}},
                {"placement": "newtab_stories_v2_4", "count": 1, "content": {"taxonomy": "IAB-3.0", "categories": ["596"]}},
            ],
        });
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_contextual_placement_flag_produces_different_hash() {
        let make_placements = || {
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "tile_1".to_string(),
            }]
        };

        let req_off = AdRequest::try_new(
            Default::default(),
            Some("same-id".to_string()),
            AdRequestFlags::default(),
            make_placements(),
            AdRequestRelevance::default(),
        )
        .unwrap();
        let req_on = AdRequest::try_new(
            Default::default(),
            Some("same-id".to_string()),
            HashMap::from([("contextual_placement".to_string(), true)]),
            make_placements(),
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert_ne!(hash(&req_off), hash(&req_on));
    }

    #[test]
    fn test_custom_env_produces_different_hash() {
        let make_placements = || {
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "tile_1".to_string(),
            }]
        };

        let request = AdRequest::try_new(
            Default::default(),
            Some("same-id".to_string()),
            AdRequestFlags::default(),
            make_placements(),
            AdRequestRelevance::default(),
        )
        .unwrap();
        let http = Environment::Custom(Url::parse("http://example.com").unwrap());
        let https = Environment::Custom(Url::parse("https://example.com").unwrap());

        assert_ne!(
            hash_for(&request, &http, false),
            hash_for(&request, &https, false)
        );
    }

    #[test]
    fn test_build_ad_request_fails_on_duplicate_placement_id() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec!["entertainment".to_string()],
                        taxonomy: IABContentTaxonomy::IAB2_1,
                    }),
                    count: 1,
                    placement: "example_placement_1".to_string(),
                },
                AdPlacementRequest {
                    content: Some(AdContentCategory {
                        categories: vec![],
                        taxonomy: IABContentTaxonomy::IAB3_0,
                    }),
                    count: 1,
                    placement: "example_placement_1".to_string(),
                },
            ],
            AdRequestRelevance::default(),
        );
        assert!(request.is_err());
    }

    #[test]
    fn test_build_ad_request_fails_on_empty_request() {
        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![],
            AdRequestRelevance::default(),
        );
        assert!(request.is_err());
    }

    #[test]
    fn test_context_id_ignored_in_hash() {
        let make_placements = || {
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "tile_1".to_string(),
            }]
        };

        let context_id_a = Some("aaaa-bbbb-cccc".to_string());
        let context_id_b = Some("dddd-eeee-ffff".to_string());

        let req1 = AdRequest::try_new(
            Default::default(),
            context_id_a,
            AdRequestFlags::default(),
            make_placements(),
            AdRequestRelevance::default(),
        )
        .unwrap();
        let req2 = AdRequest::try_new(
            Default::default(),
            context_id_b,
            AdRequestFlags::default(),
            make_placements(),
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert_eq!(hash(&req1), hash(&req2));
    }

    #[test]
    fn test_different_placements_produce_different_hash() {
        let req1 = AdRequest::try_new(
            Default::default(),
            Some("same-id".to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "tile_1".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        let req2 = AdRequest::try_new(
            Default::default(),
            Some("same-id".to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 3,
                placement: "tile_2".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert_ne!(hash(&req1), hash(&req2));
    }

    #[test]
    fn test_ohttp_flag_produces_different_hash() {
        let make_placements = || {
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "tile_1".to_string(),
            }]
        };

        let request = AdRequest::try_new(
            Default::default(),
            Some("same-id".to_string()),
            AdRequestFlags::default(),
            make_placements(),
            AdRequestRelevance::default(),
        )
        .unwrap();

        assert_ne!(
            hash_for(&request, &Environment::Test, false),
            hash_for(&request, &Environment::Test, true)
        );
    }

    #[test]
    fn test_relevance_produces_different_hash() {
        let make_request = |relevance: AdRequestRelevance| {
            AdRequest::try_new(
                Default::default(),
                Some("same-id".to_string()),
                AdRequestFlags::default(),
                vec![AdPlacementRequest {
                    content: None,
                    count: 1,
                    placement: "tile_1".to_string(),
                }],
                relevance,
            )
            .unwrap()
        };

        let req_none = make_request(AdRequestRelevance::default());
        let req_sports = make_request(HashMap::from([("sports".to_string(), 0.8)]));
        let req_sports_lower = make_request(HashMap::from([("sports".to_string(), 0.5)]));
        let req_news = make_request(HashMap::from([("news".to_string(), 0.8)]));

        assert_ne!(hash(&req_none), hash(&req_sports));
        assert_ne!(hash(&req_sports), hash(&req_sports_lower));
        assert_ne!(hash(&req_sports), hash(&req_news));
    }

    #[test]
    fn test_relevance_hash_is_independent_of_insertion_order() {
        let keys = ["arts", "business", "news", "sports", "technology", "travel"];

        let mut forward = AdRequestRelevance::new();
        for (i, k) in keys.iter().enumerate() {
            forward.insert(k.to_string(), i as f64 / 10.0);
        }
        let mut reverse = AdRequestRelevance::new();
        for (i, k) in keys.iter().enumerate().rev() {
            reverse.insert(k.to_string(), i as f64 / 10.0);
        }

        let make_request = |relevance: AdRequestRelevance| {
            AdRequest::try_new(
                Default::default(),
                Some("same-id".to_string()),
                AdRequestFlags::default(),
                vec![AdPlacementRequest {
                    content: None,
                    count: 1,
                    placement: "tile_1".to_string(),
                }],
                relevance,
            )
            .unwrap()
        };

        assert_eq!(hash(&make_request(forward)), hash(&make_request(reverse)),);
    }

    #[test]
    fn test_endpoint_const_participates_in_hash() {
        use std::collections::hash_map::DefaultHasher;

        let request = AdRequest::try_new(
            Default::default(),
            Some(TEST_CONTEXT_ID.to_string()),
            AdRequestFlags::default(),
            vec![AdPlacementRequest {
                content: None,
                count: 1,
                placement: "tile_1".to_string(),
            }],
            AdRequestRelevance::default(),
        )
        .unwrap();

        let get_ads = GetAdsRequest {
            body: &request,
            environment: &Environment::Test,
            headers: Headers::new(),
            ohttp: false,
        };

        let mut full = DefaultHasher::new();
        get_ads.hash(&mut full);

        let mut without_endpoint = DefaultHasher::new();
        get_ads.environment.hash(&mut without_endpoint);
        get_ads.ohttp.hash(&mut without_endpoint);
        get_ads.body.placements.hash(&mut without_endpoint);

        assert_ne!(
            full.finish(),
            without_endpoint.finish(),
            "ENDPOINT must contribute to GetAdsRequest hash",
        );
    }
}
