/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Follows one public operation from start to finish: the steps it went
//! through, whether it was answered from the cache, and its outcome.
//!
//! Every operation runs synchronously on the caller's thread, so the state
//! lives in a thread local and nothing has to be threaded through the clients.

use std::cell::RefCell;
use std::panic::Location;
use std::time::Instant;

use parking_lot::Mutex;

use super::classify::Classify;
use super::report::{self, ErrorSite};
use super::sink::{self, sink, Rate, RequestDuration, Sink};
use crate::http_cache::CacheOutcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    ClearCache,
    ClientBuild,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestImageAds,
    RequestSpocAds,
    RequestSpocOrImageAds,
    RequestTileAds,
}

impl Operation {
    #[cfg(test)]
    pub const ALL: [Operation; 9] = [
        Self::ClearCache,
        Self::ClientBuild,
        Self::RecordClick,
        Self::RecordImpression,
        Self::ReportAd,
        Self::RequestImageAds,
        Self::RequestSpocAds,
        Self::RequestSpocOrImageAds,
        Self::RequestTileAds,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClearCache => "clear_cache",
            Self::ClientBuild => "client_build",
            Self::RecordClick => "record_click",
            Self::RecordImpression => "record_impression",
            Self::ReportAd => "report_ad",
            Self::RequestImageAds => "request_image_ads",
            Self::RequestSpocAds => "request_spoc_ads",
            Self::RequestSpocOrImageAds => "request_spoc_or_image_ads",
            Self::RequestTileAds => "request_tile_ads",
        }
    }

    fn is_ad_request(self) -> bool {
        self.success_rate() == Some(Rate::RequestSuccess)
    }

    fn success_rate(self) -> Option<Rate> {
        match self {
            Self::RecordClick => Some(Rate::ClickSuccess),
            Self::RecordImpression => Some(Rate::ImpressionSuccess),
            Self::RequestImageAds
            | Self::RequestSpocAds
            | Self::RequestSpocOrImageAds
            | Self::RequestTileAds => Some(Rate::RequestSuccess),
            Self::ClearCache | Self::ClientBuild | Self::ReportAd => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    BuildRequest,
    Cache,
    Callback,
    ContextId,
    Open,
    Parse,
    Preflight,
    Request,
    /// Before the first step.
    Start,
}

impl Stage {
    #[cfg(test)]
    pub const ALL: [Stage; 9] = [
        Self::BuildRequest,
        Self::Cache,
        Self::Callback,
        Self::ContextId,
        Self::Open,
        Self::Parse,
        Self::Preflight,
        Self::Request,
        Self::Start,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuildRequest => "build_request",
            Self::Cache => "cache",
            Self::Callback => "callback",
            Self::ContextId => "context_id",
            Self::Open => "open",
            Self::Parse => "parse",
            Self::Preflight => "preflight",
            Self::Request => "request",
            Self::Start => "start",
        }
    }
}

struct OpState {
    cache_first: bool,
    cache_hit: bool,
    network: bool,
    ohttp: bool,
    operation: Operation,
    stage: Option<Stage>,
    steps: Vec<String>,
}

impl OpState {
    fn new(operation: Operation, ohttp: bool) -> Self {
        Self {
            cache_first: false,
            cache_hit: false,
            network: false,
            ohttp,
            operation,
            stage: None,
            steps: vec![],
        }
    }

    fn site(&self, fatal: bool, location: &'static Location<'static>) -> ErrorSite {
        ErrorSite {
            fatal,
            location: format!("{}:{}", short_path(location.file()), location.line()),
            ohttp: self.ohttp,
            operation: self.operation.as_str(),
            stage: self.stage.unwrap_or(Stage::Start).as_str(),
            steps: self.steps.clone(),
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<OpState>> = const { RefCell::new(None) };
}

/// Context ID lifetimes waiting for the end of an operation, in minutes.
static CONTEXT_ID_LIFETIMES: Mutex<Vec<i64>> = Mutex::new(Vec::new());
/// Lifetimes kept while no operation ends, e.g. a client rebuilt in a loop.
const MAX_PENDING_LIFETIMES: usize = 16;

/// Starts following a public operation. Call [`OpScope::finish`] with its
/// result.
pub fn begin(operation: Operation, ohttp: bool) -> OpScope {
    CURRENT.set(Some(OpState::new(operation, ohttp)));
    OpScope {
        operation,
        ohttp,
        started: Instant::now(),
    }
}

pub struct OpScope {
    operation: Operation,
    ohttp: bool,
    started: Instant,
}

impl OpScope {
    /// Records the operation's success metrics and its error, if any, then
    /// sends what was reported during the operation.
    ///
    /// `fill` is the number of ads returned and requested, for ad requests.
    #[track_caller]
    pub fn finish<T, E: Classify>(self, result: &Result<T, E>, fill: Option<(u32, u32)>) {
        let state = CURRENT
            .take()
            .unwrap_or_else(|| OpState::new(self.operation, self.ohttp));
        if let Err(error) = result {
            report::report(error.classify(), state.site(true, Location::caller()));
        }

        let sink = sink();
        if let Some(sink) = &sink {
            let ok = result.is_ok();
            if let Some(rate) = self.operation.success_rate() {
                sink.add_to_rate(rate, i32::from(ok), 1);
            }
            if let (true, Some((returned, requested))) = (ok, fill) {
                if requested > 0 {
                    sink.add_to_rate(Rate::Fill, saturating(returned), saturating(requested));
                }
            }
            if state.cache_first {
                sink.add_to_rate(Rate::CacheHit, i32::from(state.cache_hit && ok), 1);
            }
            if self.operation.is_ad_request() && state.network && !state.cache_hit {
                let duration = if state.ohttp {
                    RequestDuration::Ohttp
                } else {
                    RequestDuration::Direct
                };
                let millis = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
                sink.accumulate_duration(duration, millis);
            }
            send_context_id_lifetimes(sink.as_ref());
        }
        report::flush(sink.as_deref(), "error", false);
    }
}

/// Marks the step the current operation is at. Use the `step!` macro, which
/// passes the call site.
pub fn step(stage: Stage, location: &'static str) {
    CURRENT.with_borrow_mut(|state| {
        let Some(state) = state else {
            return;
        };
        state.stage = Some(stage);
        if matches!(stage, Stage::Preflight | Stage::Request | Stage::Callback) {
            state.network = true;
        }
        state
            .steps
            .push(format!("{}@{}", stage.as_str(), short_path(location)));
    });
}

/// Marks the start of an ad request's main HTTP request, as opposed to its
/// preflight.
pub fn note_main_request(cache_first: bool) {
    CURRENT.with_borrow_mut(|state| {
        if let Some(state) = state {
            state.cache_first = cache_first;
        }
    });
}

#[track_caller]
pub fn note_cache_outcome(outcome: &CacheOutcome) {
    match outcome {
        CacheOutcome::Hit => CURRENT.with_borrow_mut(|state| {
            if let Some(state) = state.as_mut().filter(|s| s.stage == Some(Stage::Request)) {
                state.cache_hit = true;
            }
        }),
        CacheOutcome::CleanupFailed(e)
        | CacheOutcome::LookupFailed(e)
        | CacheOutcome::StoreFailed(e)
        | CacheOutcome::TrimFailed(e) => report_side_error(Stage::Cache, e),
        CacheOutcome::MissNotCacheable | CacheOutcome::MissStored | CacheOutcome::NoCache => {}
    }
}

/// Reports an error that did not fail the current operation, such as an ad
/// item that was skipped. Outside an operation, it is attributed to building
/// the client.
#[track_caller]
pub fn report_side_error(stage: Stage, error: &impl Classify) {
    let location = Location::caller();
    let site = CURRENT.with_borrow(|state| match state {
        Some(state) => ErrorSite {
            stage: stage.as_str(),
            ..state.site(false, location)
        },
        None => OpState {
            stage: Some(stage),
            ..OpState::new(Operation::ClientBuild, false)
        }
        .site(false, location),
    });
    report::report(error.classify(), site);
}

/// Records how long a context ID was used, when it is replaced or when its
/// client shuts down.
pub fn record_context_id_lifetime(seconds: i64) {
    let mut lifetimes = CONTEXT_ID_LIFETIMES.lock();
    if lifetimes.len() < MAX_PENDING_LIFETIMES {
        lifetimes.push(seconds.max(0) / 60);
    }
}

fn send_context_id_lifetimes(sink: &dyn Sink) {
    for minutes in std::mem::take(&mut *CONTEXT_ID_LIFETIMES.lock()) {
        sink.accumulate_context_id_lifetime(minutes);
    }
}

/// Sends repeat counts that are still waiting for a ping, then drops the
/// sink, which UniFFI requires before shutdown.
pub fn shutdown() {
    let sink = sink();
    if let Some(sink) = &sink {
        send_context_id_lifetimes(sink.as_ref());
    }
    report::flush(sink.as_deref(), "shutdown", true);
    sink::uninstall();
}

fn saturating(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// `components/ads-client/src/mars.rs` → `mars.rs`.
fn short_path(path: &str) -> &str {
    path.rfind("src/")
        .map_or(path, |start| &path[start + "src/".len()..])
}
