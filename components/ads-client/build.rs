/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=glean/metrics.yaml");
    println!("cargo:rerun-if-changed=glean/pings.yaml");
    if env::var_os("CARGO_FEATURE_GLEAN_SYM").is_none() {
        return;
    }

    glean_build::Builder::default()
        .file("glean/metrics.yaml")
        .file("glean/pings.yaml")
        .format("rust_sym")
        .generate()
        .expect("Error generating Glean Rust bindings");

    // glean_parser's rust_sym template emits `ObjectMetric<T>`, but glean-sym's
    // `ObjectMetric` is not generic. We record objects with `set_string`.
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("glean_metrics.rs");
    let generated = fs::read_to_string(&path).expect("Error reading Glean Rust bindings");
    fs::write(&path, untype_object_metrics(&generated)).expect("Error writing Glean Rust bindings");
}

fn untype_object_metrics(generated: &str) -> String {
    const NEEDLE: &str = "ObjectMetric<";
    let mut patched = String::with_capacity(generated.len());
    let mut rest = generated;
    while let Some(start) = rest.find(NEEDLE) {
        patched.push_str(&rest[..start + NEEDLE.len() - 1]);
        let after = &rest[start + NEEDLE.len()..];
        let end = after.find('>').expect("unterminated ObjectMetric type");
        rest = &after[end + 1..];
    }
    patched.push_str(rest);
    patched
}
