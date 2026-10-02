/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

/// The callback the consumer records our metrics through, until they are
/// recorded from Rust through glean-sym. See `crate::telemetry`.
#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetry: Send + Sync {
    fn record_build_cache_error(&self, label: String, value: String);
    fn record_client_error(&self, label: String, value: String);
    fn record_client_operation_total(&self, label: String);
    fn record_deserialization_error(&self, label: String, value: String);
    fn record_http_cache_outcome(&self, label: String, value: String);
}

#[cfg(test)]
pub struct NoopMozAdsTelemetry;

#[cfg(test)]
impl MozAdsTelemetry for NoopMozAdsTelemetry {
    fn record_build_cache_error(&self, _label: String, _value: String) {}
    fn record_client_error(&self, _label: String, _value: String) {}
    fn record_client_operation_total(&self, _label: String) {}
    fn record_deserialization_error(&self, _label: String, _value: String) {}
    fn record_http_cache_outcome(&self, _label: String, _value: String) {}
}
