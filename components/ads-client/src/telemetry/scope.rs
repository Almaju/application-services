/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Follows one public operation from start to finish: the steps it went
//! through and its outcome.
//!
//! Every operation runs synchronously on the caller's thread, so the state
//! lives in a thread local and nothing has to be threaded through the clients.

use std::cell::RefCell;
use std::panic::Location;

use super::classify::Classify;
use super::report::{self, ErrorSite};
use super::sink::{self, sink};
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
    operation: Operation,
    stage: Option<Stage>,
    steps: Vec<String>,
}

impl OpState {
    fn new(operation: Operation) -> Self {
        Self {
            operation,
            stage: None,
            steps: vec![],
        }
    }

    fn site(&self, fatal: bool, location: &'static Location<'static>) -> ErrorSite {
        ErrorSite {
            fatal,
            location: format!("{}:{}", short_path(location.file()), location.line()),
            operation: self.operation.as_str(),
            stage: self.stage.unwrap_or(Stage::Start).as_str(),
            steps: self.steps.clone(),
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<OpState>> = const { RefCell::new(None) };
}

/// Starts following a public operation. Call [`OpScope::finish`] with its
/// result.
pub fn begin(operation: Operation) -> OpScope {
    CURRENT.set(Some(OpState::new(operation)));
    OpScope { operation }
}

pub struct OpScope {
    operation: Operation,
}

impl OpScope {
    /// Reports the operation's error, if any, then sends the errors reported
    /// during the operation.
    #[track_caller]
    pub fn finish<T, E: Classify>(self, result: &Result<T, E>) {
        let state = CURRENT
            .take()
            .unwrap_or_else(|| OpState::new(self.operation));
        if let Err(error) = result {
            report::report(error.classify(), state.site(true, Location::caller()));
        }
        report::flush(sink().as_deref(), "error", false);
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
        state
            .steps
            .push(format!("{}@{}", stage.as_str(), short_path(location)));
    });
}

/// Reports a failed cache read or write, which does not fail the operation.
#[track_caller]
pub fn note_cache_outcome(outcome: &CacheOutcome) {
    match outcome {
        CacheOutcome::CleanupFailed(e)
        | CacheOutcome::LookupFailed(e)
        | CacheOutcome::StoreFailed(e)
        | CacheOutcome::TrimFailed(e) => report_side_error(Stage::Cache, e),
        CacheOutcome::Hit
        | CacheOutcome::MissNotCacheable
        | CacheOutcome::MissStored
        | CacheOutcome::NoCache => {}
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
            ..OpState::new(Operation::ClientBuild)
        }
        .site(false, location),
    });
    report::report(error.classify(), site);
}

/// Sends repeat counts that are still waiting for a ping, then drops the
/// sink, which UniFFI requires before shutdown.
pub fn shutdown() {
    report::flush(sink().as_deref(), "shutdown", true);
    sink::uninstall();
}

/// `components/ads-client/src/mars.rs` → `mars.rs`.
fn short_path(path: &str) -> &str {
    path.rfind("src/")
        .map_or(path, |start| &path[start + "src/".len()..])
}
