/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Background worker for the reactive subscribe API.
//!
//! A single thread (`ads-client.worker`) owns all subscription state, so there
//! are no locks around the registries or caches. The surface talks to it only by
//! sending [`Command`]s over a channel.
//!
//! Per subscribe the worker hands over the cached value immediately (if any),
//! then fetches from MARS and emits the fresh value. Near-simultaneous subscribes
//! (a screen mounting many slots) are coalesced — we drain whatever is already
//! queued and issue one batched MARS request per ad type, rather than sleeping a
//! fixed window.
//!
//! This is a POC: each cache is an in-memory last-value map. The real V2 swaps it
//! for the SQLite-backed cache with a hard 24h TTL.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use parking_lot::Mutex;

use crate::client::error::RequestAdsError;
use crate::client::AdsClient;
use crate::ffi::telemetry::MozAdsTelemetryWrapper;
use crate::ffi::{
    MozAdsImage, MozAdsImageSubscriber, MozAdsSpoc, MozAdsSpocSubscriber, MozAdsTile,
    MozAdsTileSubscriber,
};
use crate::mars::ad_request::AdPlacementRequest;

type SharedClient = Arc<Mutex<AdsClient<MozAdsTelemetryWrapper>>>;

/// One of the three typed foreign sinks a subscription can carry.
pub(crate) enum Sink {
    Tile(Arc<dyn MozAdsTileSubscriber>),
    Image(Arc<dyn MozAdsImageSubscriber>),
    Spoc(Arc<dyn MozAdsSpocSubscriber>),
}

/// Messages the surface sends to the worker.
pub(crate) enum Command {
    Subscribe {
        id: u64,
        placement_id: String,
        sink: Sink,
    },
    Unsubscribe {
        id: u64,
    },
    Shutdown,
}

/// Spawn the worker thread. Returns the command channel and the join handle so
/// the owner can shut it down deterministically.
pub(crate) fn spawn(client: SharedClient) -> (Sender<Command>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::Builder::new()
        .name("ads-client.worker".to_string())
        .spawn(move || run(client, rx))
        .expect("failed to spawn ads-client.worker thread");
    (tx, handle)
}

fn run(client: SharedClient, rx: Receiver<Command>) {
    let mut state = WorkerState::default();
    // Block for a command, then drain the rest of the queued burst before
    // flushing, so a wave of subscribes collapses into one fetch per ad type.
    while let Ok(cmd) = rx.recv() {
        if state.handle(cmd).is_break() {
            return;
        }
        while let Ok(cmd) = rx.try_recv() {
            if state.handle(cmd).is_break() {
                return;
            }
        }
        state.flush(&client);
    }
}

/// Whether the worker loop should keep going or stop.
enum Flow {
    Continue,
    Break,
}

impl Flow {
    fn is_break(&self) -> bool {
        matches!(self, Flow::Break)
    }
}

#[derive(Default)]
struct WorkerState {
    tiles: Registry<TileKind>,
    images: Registry<ImageKind>,
    spocs: Registry<SpocKind>,
}

impl WorkerState {
    fn handle(&mut self, cmd: Command) -> Flow {
        match cmd {
            Command::Shutdown => return Flow::Break,
            Command::Unsubscribe { id } => {
                // Ids are unique across types, so at most one registry has it.
                self.tiles.remove(id);
                self.images.remove(id);
                self.spocs.remove(id);
            }
            Command::Subscribe {
                id,
                placement_id,
                sink,
            } => match sink {
                Sink::Tile(s) => self.tiles.add(id, placement_id, s),
                Sink::Image(s) => self.images.add(id, placement_id, s),
                Sink::Spoc(s) => self.spocs.add(id, placement_id, s),
            },
        }
        Flow::Continue
    }

    fn flush(&mut self, client: &SharedClient) {
        self.tiles.flush(client);
        self.images.flush(client);
        self.spocs.flush(client);
    }
}

/// Ties together the three things that differ per ad type: the foreign sink, the
/// value it receives, and how to fetch a batch of them. Everything else (the
/// registry, caching, coalescing, fan-out) is shared in [`Registry`].
trait AdKind {
    /// The foreign subscriber trait object for this ad type.
    type Sink: ?Sized + Send + Sync;
    /// What a subscriber receives. `Default` is the no-fill value (`None` / `[]`).
    type Value: Clone + Default;

    fn fetch(
        client: &AdsClient<MozAdsTelemetryWrapper>,
        placements: Vec<AdPlacementRequest>,
    ) -> Result<HashMap<String, Self::Value>, RequestAdsError>;

    fn on_ads(sink: &Self::Sink, value: Self::Value);
}

/// Placement -> subscribers -> last-value cache for one ad type.
struct Registry<K: AdKind> {
    subscribers: HashMap<u64, (String, Arc<K::Sink>)>,
    by_placement: HashMap<String, HashSet<u64>>,
    cache: HashMap<String, K::Value>,
    /// Placements with a new subscriber since the last flush.
    dirty: HashSet<String>,
}

impl<K: AdKind> Default for Registry<K> {
    fn default() -> Self {
        Self {
            subscribers: HashMap::new(),
            by_placement: HashMap::new(),
            cache: HashMap::new(),
            dirty: HashSet::new(),
        }
    }
}

impl<K: AdKind> Registry<K> {
    fn add(&mut self, id: u64, placement: String, sink: Arc<K::Sink>) {
        // Hand over the cached value immediately so the surface renders without
        // waiting on the network. No cache hit = nothing until the flush below.
        if let Some(value) = self.cache.get(&placement) {
            K::on_ads(&sink, value.clone());
        }
        self.by_placement
            .entry(placement.clone())
            .or_default()
            .insert(id);
        self.subscribers.insert(id, (placement.clone(), sink));
        self.dirty.insert(placement);
    }

    fn remove(&mut self, id: u64) {
        if let Some((placement, _)) = self.subscribers.remove(&id) {
            if let Some(ids) = self.by_placement.get_mut(&placement) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.by_placement.remove(&placement);
                }
            }
        }
    }

    /// Fetch every dirty placement that still has subscribers in one batched
    /// request, then fan the typed result out to them.
    fn flush(&mut self, client: &SharedClient) {
        let placements: Vec<String> = self
            .dirty
            .drain()
            .filter(|p| self.by_placement.contains_key(p))
            .collect();
        if placements.is_empty() {
            return;
        }

        let requests = placements
            .iter()
            .map(|placement| AdPlacementRequest {
                content: None,
                count: 1,
                placement: placement.clone(),
            })
            .collect();

        // On failure the fetch already recorded telemetry; there's nothing to
        // deliver, so subscribers keep their last value until the next refresh.
        if let Ok(mut fetched) = K::fetch(&client.lock(), requests) {
            for placement in &placements {
                let value = fetched.remove(placement).unwrap_or_default();
                self.cache.insert(placement.clone(), value.clone());
                self.for_each(placement, |sink| K::on_ads(sink, value.clone()));
            }
        }
    }

    fn for_each(&self, placement: &str, f: impl Fn(&K::Sink)) {
        if let Some(ids) = self.by_placement.get(placement) {
            for id in ids {
                if let Some((_, sink)) = self.subscribers.get(id) {
                    f(sink.as_ref());
                }
            }
        }
    }
}

struct TileKind;
impl AdKind for TileKind {
    type Sink = dyn MozAdsTileSubscriber;
    type Value = Option<MozAdsTile>;

    fn fetch(
        client: &AdsClient<MozAdsTelemetryWrapper>,
        placements: Vec<AdPlacementRequest>,
    ) -> Result<HashMap<String, Self::Value>, RequestAdsError> {
        Ok(client
            .request_tile_ads(placements, None, false)?
            .into_iter()
            .map(|(placement, ad)| (placement, Some(ad.into())))
            .collect())
    }

    fn on_ads(sink: &Self::Sink, value: Self::Value) {
        sink.on_ads(value);
    }
}

struct ImageKind;
impl AdKind for ImageKind {
    type Sink = dyn MozAdsImageSubscriber;
    type Value = Option<MozAdsImage>;

    fn fetch(
        client: &AdsClient<MozAdsTelemetryWrapper>,
        placements: Vec<AdPlacementRequest>,
    ) -> Result<HashMap<String, Self::Value>, RequestAdsError> {
        Ok(client
            .request_image_ads(placements, None, false)?
            .into_iter()
            .map(|(placement, ad)| (placement, Some(ad.into())))
            .collect())
    }

    fn on_ads(sink: &Self::Sink, value: Self::Value) {
        sink.on_ads(value);
    }
}

struct SpocKind;
impl AdKind for SpocKind {
    type Sink = dyn MozAdsSpocSubscriber;
    type Value = Vec<MozAdsSpoc>;

    fn fetch(
        client: &AdsClient<MozAdsTelemetryWrapper>,
        placements: Vec<AdPlacementRequest>,
    ) -> Result<HashMap<String, Self::Value>, RequestAdsError> {
        Ok(client
            .request_spoc_ads(placements, None, false)?
            .into_iter()
            .map(|(placement, ads)| (placement, ads.into_iter().map(Into::into).collect()))
            .collect())
    }

    fn on_ads(sink: &Self::Sink, value: Self::Value) {
        sink.on_ads(value);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{channel, Sender};
    use std::time::Duration;

    use super::*;
    use crate::client::config::AdsClientConfig;
    use crate::mars::Environment;
    use crate::test_utils::get_example_happy_uatile_response;

    /// Test sink that forwards every tile emission onto a channel.
    struct ChannelSink(Sender<Option<MozAdsTile>>);
    impl MozAdsTileSubscriber for ChannelSink {
        fn on_ads(&self, ad: Option<MozAdsTile>) {
            let _ = self.0.send(ad);
        }
    }

    fn test_client() -> SharedClient {
        let config = AdsClientConfig {
            cache_config: None,
            context_id_provider: None,
            environment: Environment::Test,
            telemetry: MozAdsTelemetryWrapper::noop(),
        };
        Arc::new(Mutex::new(AdsClient::new(config)))
    }

    #[test]
    fn test_subscribe_emits_live_ad_then_serves_cache_to_next_subscriber() {
        viaduct_dev::init_backend_dev();
        let expected = get_example_happy_uatile_response();
        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected.data).unwrap())
            .expect_at_least(1)
            .create();

        let (tx, handle) = spawn(test_client());

        // First subscriber: cold cache, so the only emission is the live ad.
        let (first_tx, first_rx) = channel();
        tx.send(Command::Subscribe {
            id: 1,
            placement_id: "example_placement_1".to_string(),
            sink: Sink::Tile(Arc::new(ChannelSink(first_tx))),
        })
        .unwrap();

        let live = first_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(live.is_some(), "{live:?}");

        // Second subscriber to the same placement: it gets the cached ad
        // immediately, then the live ad from its own fetch — two emissions,
        // proving the cache-then-live double-fire.
        let (second_tx, second_rx) = channel();
        tx.send(Command::Subscribe {
            id: 2,
            placement_id: "example_placement_1".to_string(),
            sink: Sink::Tile(Arc::new(ChannelSink(second_tx))),
        })
        .unwrap();

        let cached = second_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let refreshed = second_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(cached.is_some(), "{cached:?}");
        assert!(refreshed.is_some(), "{refreshed:?}");

        tx.send(Command::Shutdown).unwrap();
        handle.join().unwrap();
    }
}
