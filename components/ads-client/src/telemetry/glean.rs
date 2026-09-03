/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The glean-sym backend: one function per metric in `metrics.yaml`.
//!
//! `glean-sym` finds Glean's FFI entry points in the host application at
//! runtime, so these calls land in the same Glean instance — and therefore the
//! same `metrics` ping — as everything the app records itself. If Glean has not
//! been initialized yet, the recording is buffered by Glean's own dispatcher
//! exactly as it would be for a metric recorded from Kotlin or Swift.

use crate::glean_metrics::ads_client;

pub(super) fn build_cache_error(label: &str, value: String) {
    ads_client::build_cache_error.get(label).set(value);
}

pub(super) fn client_error(label: &str, value: String) {
    ads_client::client_error.get(label).set(value);
}

pub(super) fn client_operation_total(label: &str) {
    ads_client::client_operation_total.get(label).add(1);
}

pub(super) fn deserialization_error(label: &str, value: String) {
    ads_client::deserialization_error.get(label).set(value);
}

pub(super) fn http_cache_outcome(label: &str, value: String) {
    ads_client::http_cache_outcome.get(label).set(value);
}
