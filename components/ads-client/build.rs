/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{env, fs, path::PathBuf};

use glean_build::Builder;

// The line the `rust_sym` template emits at the top of every category module.
const GENERATED_IMPORT: &str = "use glean_sym::{metrics::*, types::*};";

fn main() {
    println!("cargo:rerun-if-changed=metrics.yaml");
    println!("cargo:rerun-if-env-changed=MOZ_TOPOBJDIR");
    println!("cargo:rustc-check-cfg=cfg(glean_sym)");

    // One decision, in one place: `cfg(glean_sym)` is the only thing the crate
    // itself checks.
    if !glean_sym_enabled() {
        return;
    }
    println!("cargo:rustc-cfg=glean_sym");

    Builder::default()
        .file("metrics.yaml")
        .format("rust_sym")
        .generate()
        .expect("Error generating Glean Rust bindings");

    patch_labeled_metric_import();
}

/// Whether to record metrics through glean-sym for this build.
///
/// glean-sym looks Glean's FFI entry points up in the process at runtime and
/// panics if it cannot find them, so this must be true only where we are
/// certain we are running inside an application that embeds Glean.
fn glean_sym_enabled() -> bool {
    if env::var_os("CARGO_FEATURE_GLEAN_SYM").is_none() {
        return false;
    }

    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        // The megazord is loaded alongside the app's own Glean.
        Ok("android" | "ios") => true,
        // On desktop we are linked into libxul, so Glean is in the same
        // process — but only when gecko is the one building us. A standalone
        // `cargo build` of this crate has no Glean to talk to.
        Ok("linux" | "macos") => is_gecko_build(),
        // Windows is missing: glean-sym does not compile there
        // (`compile_error!("This crate is not implemented for Windows")`).
        _ => false,
    }
}

/// Whether gecko is building us, by the same signal `rc_crypto` uses.
fn is_gecko_build() -> bool {
    env::var_os("MOZ_TOPOBJDIR").is_some()
}

/// Bring our `LabeledMetric` stand-in into scope in the generated module.
///
/// Every metric in `metrics.yaml` is labeled, and for those the `rust_sym`
/// template emits `LabeledMetric<_>` / `LabeledMetricData`, which glean-sym
/// itself does not define (as of v68.0.0 — v70.0.0 too). The generated module
/// only imports `glean_sym::{metrics::*, types::*}`, so it cannot see the
/// substitutes in `telemetry::labeled` unless we add the import here.
///
/// Delete this — and `telemetry::labeled` with it — once glean-sym exposes
/// labeled metrics of its own.
fn patch_labeled_metric_import() {
    let generated = PathBuf::from(env::var("OUT_DIR").unwrap()).join("glean_metrics.rs");
    let source = fs::read_to_string(&generated).expect("Generated Glean bindings not readable");
    assert!(
        source.contains(GENERATED_IMPORT),
        "glean_parser no longer emits `{GENERATED_IMPORT}`; \
         re-check whether the LabeledMetric shim is still needed"
    );
    let patched = source.replace(
        GENERATED_IMPORT,
        &format!(
            "{GENERATED_IMPORT}\n    use crate::telemetry::labeled::{{LabeledMetric, LabeledMetricData}};"
        ),
    );
    fs::write(&generated, patched).expect("Could not write patched Glean bindings");
}
