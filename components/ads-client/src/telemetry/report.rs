/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Holds unexpected errors until the end of the operation, then sends them as
//! records in the `ads-client-errors` ping. Expected errors are left to the
//! `MozAdsTelemetry` metrics.
//!
//! Nothing is sent while an error is being reported: errors raised while the
//! client is built would otherwise reach Glean before the app has loaded it.

use std::collections::HashMap;
use std::sync::LazyLock;

use parking_lot::Mutex;
use serde::Serialize;

use super::classify::{Classified, Tier};
use super::sink::Sink;

/// Distinct unexpected errors per process. Later ones only count as dropped.
const MAX_FINGERPRINTS: usize = 10;
/// `ads-client-errors` pings per process.
const MAX_PINGS: u32 = 5;
const MAX_STEPS: usize = 16;

/// Where and when an error happened.
pub struct ErrorSite {
    pub fatal: bool,
    pub location: String,
    pub operation: &'static str,
    pub stage: &'static str,
    pub steps: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct Record {
    fp: String,
    kind: &'static str,
    op: &'static str,
    stage: &'static str,
    fatal: bool,
    chain: Vec<&'static str>,
    message: String,
    location: String,
    steps: Vec<String>,
    context: Vec<ContextEntry>,
    count: u32,
}

#[derive(Clone, Debug, Serialize)]
struct ContextEntry {
    k: &'static str,
    v: String,
}

#[derive(Serialize)]
struct Payload<'a> {
    v: u32,
    dropped: u32,
    records: &'a [Record],
}

#[derive(Default)]
struct Reporter {
    dropped: u32,
    /// Errors seen since the last ping, one entry per fingerprint.
    pending: Vec<Record>,
    pending_new: bool,
    pings: u32,
    /// The first record of every fingerprint seen in this process.
    seen: HashMap<String, Record>,
}

// By design: process-wide, so two clients share the per-process caps.
static REPORTER: LazyLock<Mutex<Reporter>> = LazyLock::new(Default::default);

pub fn report(classified: Classified, site: ErrorSite) {
    if classified.tier() == Tier::Unexpected {
        REPORTER.lock().add(classified, site);
    }
}

/// Sends what was reported since the last call to `sink`. Without a sink,
/// errors stay pending. The ping only goes out when a new error was seen, or
/// when `force` is set.
pub fn flush(sink: Option<&dyn Sink>, reason: &'static str, force: bool) {
    let Some(sink) = sink else {
        return;
    };
    let mut reporter = REPORTER.lock();
    let due = reporter.pending_new || (force && !reporter.pending.is_empty());
    if !due || reporter.pings >= MAX_PINGS {
        return;
    }
    let payload = Payload {
        v: 1,
        dropped: reporter.dropped,
        records: &reporter.pending,
    };
    let Ok(json) = serde_json::to_string(&payload) else {
        return;
    };
    reporter.pending.clear();
    reporter.pending_new = false;
    reporter.pings += 1;
    // By design: sent while holding the lock, so two operations ending at
    // once cannot overwrite each other's records before the ping is sent.
    sink.submit_error_reports(json, reason);
}

impl Reporter {
    fn add(&mut self, classified: Classified, site: ErrorSite) {
        let fp = fingerprint(&classified, &site);
        if let Some(pending) = self.pending.iter_mut().find(|r| r.fp == fp) {
            pending.count += 1;
            return;
        }
        if let Some(first) = self.seen.get(&fp) {
            self.pending.push(Record {
                count: 1,
                ..first.clone()
            });
            return;
        }
        if self.seen.len() >= MAX_FINGERPRINTS {
            self.dropped += 1;
            return;
        }
        let mut steps = site.steps;
        steps.truncate(MAX_STEPS);
        let record = Record {
            fp: fp.clone(),
            kind: classified.kind.as_str(),
            op: site.operation,
            stage: site.stage,
            fatal: site.fatal,
            chain: classified.chain,
            message: classified.message.into_owned(),
            location: site.location,
            steps,
            context: classified
                .context
                .into_iter()
                .chain(classified.code.map(|code| ("code", code)))
                .map(|(k, v)| ContextEntry { k, v })
                .collect(),
            count: 1,
        };
        self.seen.insert(fp, record.clone());
        self.pending.push(record);
        self.pending_new = true;
    }
}

/// Identifies an error across clients and releases: no line numbers, no
/// wrapper variants and no free text, but the values that tell root causes
/// apart, such as the field that failed to deserialize.
fn fingerprint(classified: &Classified, site: &ErrorSite) -> String {
    let serde_path = classified
        .context
        .iter()
        .find(|(k, _)| *k == "serde_path")
        .map(|(_, v)| v.as_str());
    let parts = [
        classified.kind.as_str(),
        site.operation,
        site.stage,
        classified.leaf(),
        classified.code.as_deref().unwrap_or_default(),
        serde_path.unwrap_or_default(),
    ];
    // FNV-1a, 64 bits.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in parts.join("|").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
pub fn reset() {
    *REPORTER.lock() = Reporter::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::classify::ErrorKind;
    use crate::telemetry::sink::Capture;

    fn site() -> ErrorSite {
        ErrorSite {
            fatal: true,
            location: "lib.rs:1".into(),
            operation: "request_spoc_ads",
            stage: "parse",
            steps: vec![],
        }
    }

    fn unexpected(path: &str) -> Classified {
        Classified::new(ErrorKind::InvalidAdItem, "AdResponse::invalid_item", "x")
            .context("serde_path", path)
    }

    #[test]
    fn test_new_errors_ping_and_repeats_wait() {
        let _lock = crate::telemetry::test_lock();
        reset();
        let capture = Capture::default();

        report(unexpected("callbacks"), site());
        report(unexpected("callbacks"), site());
        flush(Some(&capture), "error", false);
        let reports = capture.error_reports.lock().clone();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0["records"][0]["count"], 2);

        // A repeat alone does not send a ping, shutdown does.
        report(unexpected("callbacks"), site());
        flush(Some(&capture), "error", false);
        assert_eq!(capture.error_reports.lock().len(), 1);
        flush(Some(&capture), "shutdown", true);
        let reports = capture.error_reports.lock().clone();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[1].1, "shutdown");
        assert_eq!(reports[1].0["records"][0]["count"], 1);
    }

    #[test]
    fn test_caps() {
        let _lock = crate::telemetry::test_lock();
        reset();
        let capture = Capture::default();

        for i in 0..MAX_FINGERPRINTS + 3 {
            report(unexpected(&format!("field{i}")), site());
        }
        flush(Some(&capture), "error", false);
        let reports = capture.error_reports.lock().clone();
        assert_eq!(reports[0].0["dropped"], 3);
        assert_eq!(
            reports[0].0["records"].as_array().unwrap().len(),
            MAX_FINGERPRINTS
        );
    }

    #[test]
    fn test_expected_errors_are_not_reported() {
        let _lock = crate::telemetry::test_lock();
        reset();
        let capture = Capture::default();

        report(
            Classified::new(ErrorKind::Timeout, "HTTPError::Unexpected", "x"),
            site(),
        );
        flush(Some(&capture), "error", true);
        assert!(capture.error_reports.lock().is_empty());
    }

    #[test]
    fn test_fingerprint_tells_fields_apart() {
        let a = fingerprint(&unexpected("callbacks.click"), &site());
        let b = fingerprint(&unexpected("image_url"), &site());
        assert_ne!(a, b);
        assert_eq!(a, fingerprint(&unexpected("callbacks.click"), &site()));
    }
}
