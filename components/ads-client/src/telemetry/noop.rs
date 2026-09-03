/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The backend for builds with no Glean to talk to: a standalone `cargo build`
//! or `cargo test`, Windows (where glean-sym does not compile), and anything
//! built with `--no-default-features`. See `glean_sym_enabled` in `build.rs`.
//!
//! Recording sites stay in the code and stay type-checked; the values are
//! dropped here.

pub(super) fn build_cache_error(_label: &str, _value: String) {}

pub(super) fn client_error(_label: &str, _value: String) {}

pub(super) fn client_operation_total(_label: &str) {}

pub(super) fn deserialization_error(_label: &str, _value: String) {}

pub(super) fn http_cache_outcome(_label: &str, _value: String) {}
