/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{env, fs, path::PathBuf};

use glean_build::Builder;

// The line the `rust_sym` template emits at the top of every category module.
const GENERATED_IMPORT: &str = "use glean_sym::{metrics::*, types::*};";

fn main() {
    println!("cargo:rerun-if-changed=metrics.yaml");

    // glean-sym is a mobile-only dependency, so only generate metrics there.
    // Everywhere else `telemetry::backend` is the no-op and nothing includes
    // the generated file.
    if !matches!(
        env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("android" | "ios")
    ) {
        return;
    }

    Builder::default()
        .file("metrics.yaml")
        .format("rust_sym")
        .generate()
        .expect("Error generating Glean Rust bindings");

    patch_labeled_metric_import();
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
