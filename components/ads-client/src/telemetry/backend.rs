/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The telemetry backend: one function per metric in `metrics.yaml`, each
//! forwarded to the process-wide [`Telemetry`] implementation.
//!
//! The function signatures match the glean-sym backend this will be replaced
//! with, so the swap only touches this file. [`Telemetry`], [`install`] and
//! [`uninstall`] exist only because the FFI layer hands us a UniFFI callback
//! that has to be dropped again before shutdown; glean-sym needs none of them.
//!
//! Until a callback is installed, and after it is uninstalled, recording is a
//! no-op.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::bytesize::ByteSize;

/// Receives the ads client's metrics.
pub trait Telemetry: Send + Sync {
    fn record_client_error(&self, label: &str, value: String);
    fn record_client_operation_total(&self, label: &str);
    fn record_http_cache_outcome(&self, label: &str, value: String);
    fn record_unexpected_error(&self, kind: &str, message: String);
}

// By design: telemetry is a process-wide singleton, as it will be with glean-sym.
static CALLBACK: RwLock<Option<Arc<dyn Telemetry>>> = RwLock::new(None);

/// Makes `callback` the destination of every metric recorded from now on.
pub fn install(callback: impl Telemetry + 'static) {
    // Dropping a UniFFI callback calls into foreign code, so let the replaced
    // one drop after the lock is released.
    let _replaced = CALLBACK.write().replace(Arc::new(callback));
}

/// Drops the installed callback, after which recording is a no-op.
///
/// UniFFI callbacks left hanging at shutdown crash Firefox Desktop on quit, so
/// the client uninstalls the callback when it shuts down.
pub fn uninstall() {
    // As in `install`, the removed callback drops after the lock is released.
    let _removed = CALLBACK.write().take();
}

fn with_callback(f: impl FnOnce(&dyn Telemetry)) {
    // Clone out of the lock so a slow callback never holds up `install` or
    // `uninstall`.
    let Some(callback) = CALLBACK.read().clone() else {
        return;
    };
    f(callback.as_ref());
}

/// Glean drops the end of longer string values and records an
/// `invalid_overflow` error (bug 2047640).
const MAX_STRING_SIZE: ByteSize = ByteSize::b(255);

/// Longest `ads_client.unexpected_error` message.
const MAX_MESSAGE_SIZE: ByteSize = ByteSize::kib(1);

/// Cuts `value` to `max_size`, on a char boundary.
fn fit(mut value: String, max_size: ByteSize) -> String {
    let max = max_size.as_u64() as usize;
    if value.len() > max {
        // `str::floor_char_boundary` does this, but needs Rust 1.91 and
        // mozilla-central still builds with 1.90.
        let mut end = max;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

pub(super) fn client_error(label: &str, value: String) {
    with_callback(|cb| cb.record_client_error(label, fit(value, MAX_STRING_SIZE)));
}

pub(super) fn client_operation_total(label: &str) {
    with_callback(|cb| cb.record_client_operation_total(label));
}

pub(super) fn http_cache_outcome(label: &str, value: String) {
    with_callback(|cb| cb.record_http_cache_outcome(label, fit(value, MAX_STRING_SIZE)));
}

pub(super) fn unexpected_error(kind: &str, message: String) {
    with_callback(|cb| cb.record_unexpected_error(kind, fit(message, MAX_MESSAGE_SIZE)));
}

/// Serializes tests that install a callback, since they all share the one
/// singleton.
#[cfg(test)]
pub fn test_lock() -> parking_lot::MutexGuard<'static, ()> {
    static LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    LOCK.lock()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{record_client_operation, record_invalid_ad_item, ClientOperation};
    use parking_lot::Mutex;

    #[derive(Clone, Default)]
    struct RecordingTelemetry(Arc<Mutex<Vec<String>>>);

    impl Telemetry for RecordingTelemetry {
        fn record_client_error(&self, _label: &str, _value: String) {}
        fn record_client_operation_total(&self, label: &str) {
            self.0.lock().push(label.to_string());
        }
        fn record_http_cache_outcome(&self, _label: &str, _value: String) {}
        fn record_unexpected_error(&self, kind: &str, message: String) {
            self.0.lock().push(format!("{kind}: {message}"));
        }
    }

    #[test]
    fn test_fit_cuts_on_a_char_boundary() {
        assert_eq!(fit("short".into(), MAX_STRING_SIZE), "short");
        assert_eq!(
            fit("a".repeat(300), MAX_STRING_SIZE).len() as u64,
            MAX_STRING_SIZE.as_u64()
        );
        let cut = fit("é".repeat(200), MAX_STRING_SIZE);
        assert_eq!(cut.len(), 254);
        assert!(cut.chars().all(|c| c == 'é'));
    }

    #[test]
    fn test_records_unexpected_errors() {
        let _lock = test_lock();
        let recording = RecordingTelemetry::default();
        install(recording.clone());

        let error = serde_json::from_str::<u32>(&format!("\"{}\"", "x".repeat(2000))).unwrap_err();
        record_invalid_ad_item(&error);

        let recorded = recording.0.lock().clone();
        let message = recorded
            .iter()
            .find_map(|r| r.strip_prefix("invalid_ad_item: "))
            .expect("the error is recorded");
        assert!(message.starts_with("invalid type: string"));
        assert_eq!(message.len() as u64, MAX_MESSAGE_SIZE.as_u64());
        uninstall();
    }

    #[test]
    fn test_records_to_installed_callback() {
        let _lock = test_lock();
        let recording = RecordingTelemetry::default();
        install(recording.clone());

        record_client_operation(ClientOperation::ReportAd);
        // Other tests may record concurrently, so only look for ours.
        assert!(recording.0.lock().iter().any(|label| label == "report_ad"));

        uninstall();
        recording.0.lock().clear();
        record_client_operation(ClientOperation::ReportAd);
        assert!(recording.0.lock().is_empty());
    }
}
