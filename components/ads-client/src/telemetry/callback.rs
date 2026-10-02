/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The callback backend: one function per metric in `metrics.yaml`, each
//! forwarded to the process-wide [`MozAdsTelemetry`] callback.
//!
//! The function signatures match the glean-sym backend this will be replaced
//! with, so the swap only touches this file. [`install`] and [`uninstall`]
//! exist only because a UniFFI callback has to be handed to us and dropped
//! again before shutdown; glean-sym needs neither and they go away with it.
//!
//! Until a callback is installed, and after it is uninstalled, recording is a
//! no-op.

use std::sync::{Arc, Weak};

use parking_lot::RwLock;

use crate::ffi::telemetry::MozAdsTelemetry;

static CALLBACK: RwLock<Option<Arc<dyn MozAdsTelemetry>>> = RwLock::new(None);

/// Makes `callback` the destination of every metric recorded from now on,
/// replacing any callback installed before it.
pub fn install(callback: Arc<dyn MozAdsTelemetry>) {
    *CALLBACK.write() = Some(callback);
}

/// Drops the installed callback if it is `callback`, after which recording is
/// a no-op until another one is installed.
///
/// UniFFI callbacks left hanging at shutdown crash Firefox Desktop on quit, so
/// each client drops the one it installed when it shuts down. A callback that
/// has since been replaced by another client's is left alone.
pub fn uninstall(callback: &Weak<dyn MozAdsTelemetry>) {
    let mut installed = CALLBACK.write();
    if installed
        .as_ref()
        .is_some_and(|current| Weak::ptr_eq(&Arc::downgrade(current), callback))
    {
        *installed = None;
    }
}

fn with_callback(f: impl FnOnce(&dyn MozAdsTelemetry)) {
    // Clone out of the lock so a slow callback never holds up `install` or
    // `uninstall`.
    let Some(callback) = CALLBACK.read().clone() else {
        return;
    };
    f(callback.as_ref());
}

pub(super) fn build_cache_error(label: &str, value: String) {
    with_callback(|cb| cb.record_build_cache_error(label.to_string(), value));
}

pub(super) fn client_error(label: &str, value: String) {
    with_callback(|cb| cb.record_client_error(label.to_string(), value));
}

pub(super) fn client_operation_total(label: &str) {
    with_callback(|cb| cb.record_client_operation_total(label.to_string()));
}

pub(super) fn deserialization_error(label: &str, value: String) {
    with_callback(|cb| cb.record_deserialization_error(label.to_string(), value));
}

pub(super) fn http_cache_outcome(label: &str, value: String) {
    with_callback(|cb| cb.record_http_cache_outcome(label.to_string(), value));
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
    use crate::ffi::telemetry::NoopMozAdsTelemetry;
    use crate::telemetry::{record_client_operation, ClientOperation};
    use parking_lot::Mutex;

    #[derive(Default)]
    struct RecordingTelemetry(Mutex<Vec<String>>);

    impl MozAdsTelemetry for RecordingTelemetry {
        fn record_build_cache_error(&self, _label: String, _value: String) {}
        fn record_client_error(&self, _label: String, _value: String) {}
        fn record_client_operation_total(&self, label: String) {
            self.0.lock().push(label);
        }
        fn record_deserialization_error(&self, _label: String, _value: String) {}
        fn record_http_cache_outcome(&self, _label: String, _value: String) {}
    }

    #[test]
    fn test_records_to_installed_callback() {
        let _lock = test_lock();
        let recording = Arc::new(RecordingTelemetry::default());
        let callback: Arc<dyn MozAdsTelemetry> = recording.clone();
        install(callback.clone());

        record_client_operation(ClientOperation::ReportAd);
        // Other tests may record concurrently, so only look for ours.
        assert!(recording.0.lock().iter().any(|label| label == "report_ad"));

        uninstall(&Arc::downgrade(&callback));
        recording.0.lock().clear();
        record_client_operation(ClientOperation::ReportAd);
        assert!(recording.0.lock().is_empty());
    }

    #[test]
    fn test_uninstall_leaves_a_replacement_alone() {
        let _lock = test_lock();
        let first: Arc<dyn MozAdsTelemetry> = Arc::new(NoopMozAdsTelemetry);
        let second: Arc<dyn MozAdsTelemetry> = Arc::new(NoopMozAdsTelemetry);
        let first_weak = Arc::downgrade(&first);
        let second_weak = Arc::downgrade(&second);

        install(first);
        install(second);
        // Replacing a callback drops it.
        assert_eq!(first_weak.strong_count(), 0);

        uninstall(&first_weak);
        assert_eq!(second_weak.strong_count(), 1);

        uninstall(&second_weak);
        assert_eq!(second_weak.strong_count(), 0);
    }
}
