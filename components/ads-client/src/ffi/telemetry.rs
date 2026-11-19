/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

/// Deprecated: the client records its metrics itself and never calls this.
/// It stays until apps stop passing one to `MozAdsClientBuilder::telemetry()`.
#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetry: Send + Sync {
    /// The HTTP cache could not be built (`ads_client.build_cache_error`).
    fn record_build_cache_error(&self, label: String, value: String);
    /// A client operation failed (`ads_client.client_error`).
    fn record_client_error(&self, label: String, value: String);
    /// A client operation was attempted (`ads_client.client_operation_total`).
    fn record_client_operation_total(&self, label: String);
    /// An ad in a MARS response could not be parsed and was skipped
    /// (`ads_client.deserialization_error`).
    fn record_deserialization_error(&self, label: String, value: String);
    /// The outcome of a read from the HTTP cache (`ads_client.http_cache_outcome`).
    fn record_http_cache_outcome(&self, label: String, value: String);
}
