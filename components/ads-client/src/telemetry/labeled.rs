/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! A stand-in for glean-core's `LabeledMetric`, built out of what glean-sym has.
//!
//! `glean_parser`'s `rust_sym` output refers to `LabeledMetric<_>` and
//! `LabeledMetricData` for any labeled metric, but glean-sym exposes no labeled
//! metric types (checked against v68.0.0 and v70.0.0 — only
//! `DualLabeledCounterMetric` is there). Since every metric in our
//! `metrics.yaml` is labeled, the generated module does not compile without
//! these two names in scope; `build.rs` adds the import that puts them there.
//!
//! What follows is a reimplementation of the part of glean-core that matters
//! here, so the recorded data is indistinguishable from what the Kotlin and
//! Swift bindings produce today:
//!
//! - a metric with a static label list stores each submetric as an ordinary
//!   metric named `"<metric>/<label>"` in the same category, with labels
//!   outside the list folded into `__other__`;
//! - a metric without one records through `dynamic_label`, leaving the label
//!   validation and the 16-label cap to Glean itself.
//!
//! Delete this module, and the patching step in `build.rs`, once glean-sym
//! grows labeled metrics of its own.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use glean_sym::metrics::{CounterMetric, StringMetric};
use glean_sym::types::{CommonMetricData, DynamicLabelType, Lifetime};
use parking_lot::Mutex;

/// The label an unrecognized label is recorded under. Matches glean-core.
const OTHER_LABEL: &str = "__other__";

/// The metric data a labeled metric needs to build its submetrics.
///
/// glean-core has a variant per metric kind that takes extra parameters
/// (custom, memory and timing distributions). We only have labeled counters and
/// labeled strings, so only the common case is here — a labeled distribution in
/// `metrics.yaml` will fail to compile rather than record something wrong.
pub enum LabeledMetricData {
    Common { cmd: CommonMetricData },
}

/// A metric that can be a labeled metric's submetric.
pub trait LabeledSubmetric {
    fn from_meta(meta: CommonMetricData) -> Self;
}

impl LabeledSubmetric for CounterMetric {
    fn from_meta(meta: CommonMetricData) -> Self {
        CounterMetric::new(meta)
    }
}

impl LabeledSubmetric for StringMetric {
    fn from_meta(meta: CommonMetricData) -> Self {
        StringMetric::new(meta)
    }
}

pub struct LabeledMetric<T> {
    meta: SubmetricMeta,
    labels: Option<Vec<Cow<'static, str>>>,
    /// Submetrics by label. Glean identifies a metric by an integer handle
    /// obtained from its FFI constructor, and glean-sym never releases one, so
    /// each submetric is built once and kept.
    submetrics: Mutex<HashMap<String, Arc<T>>>,
}

impl<T: LabeledSubmetric> LabeledMetric<T> {
    pub fn new(meta: LabeledMetricData, labels: Option<Vec<Cow<'static, str>>>) -> Self {
        let LabeledMetricData::Common { cmd } = meta;
        Self {
            meta: SubmetricMeta::from(cmd),
            labels,
            submetrics: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the submetric for `label`, creating it on first use.
    pub fn get(&self, label: &str) -> Arc<T> {
        // With a static label list, anything we don't recognize goes to
        // `__other__`. Without one, Glean validates and caps the label when it
        // records, so pass it through untouched.
        let label = match &self.labels {
            Some(labels) if !labels.iter().any(|known| known == label) => OTHER_LABEL,
            _ => label,
        };

        let mut submetrics = self.submetrics.lock();
        Arc::clone(submetrics.entry(label.to_string()).or_insert_with(|| {
            let meta = match self.labels {
                // A statically labeled submetric is an ordinary metric named
                // after the label.
                Some(_) => self.meta.with_static_label(label),
                None => self.meta.with_dynamic_label(label),
            };
            Arc::new(T::from_meta(meta))
        }))
    }
}

/// The pieces of a `CommonMetricData` a labeled metric has to hand to each of
/// its submetrics. Kept separately because `CommonMetricData` is a UniFFI
/// record and is neither `Clone` nor `Copy`.
struct SubmetricMeta {
    category: String,
    name: String,
    send_in_pings: Vec<String>,
    lifetime: MetricLifetime,
    disabled: bool,
    in_session: bool,
}

impl SubmetricMeta {
    fn with_static_label(&self, label: &str) -> CommonMetricData {
        self.build(format!("{}/{}", self.name, label), None)
    }

    fn with_dynamic_label(&self, label: &str) -> CommonMetricData {
        self.build(
            self.name.clone(),
            Some(DynamicLabelType::Label(label.to_string())),
        )
    }

    fn build(&self, name: String, dynamic_label: Option<DynamicLabelType>) -> CommonMetricData {
        CommonMetricData {
            category: self.category.clone(),
            name,
            send_in_pings: self.send_in_pings.clone(),
            lifetime: self.lifetime.into(),
            disabled: self.disabled,
            dynamic_label,
            in_session: self.in_session,
        }
    }
}

impl From<CommonMetricData> for SubmetricMeta {
    fn from(cmd: CommonMetricData) -> Self {
        Self {
            category: cmd.category,
            name: cmd.name,
            send_in_pings: cmd.send_in_pings,
            lifetime: cmd.lifetime.into(),
            disabled: cmd.disabled,
            in_session: cmd.in_session,
        }
    }
}

/// A copyable `glean_sym::types::Lifetime`.
#[derive(Clone, Copy)]
enum MetricLifetime {
    Ping,
    Application,
    User,
}

impl From<Lifetime> for MetricLifetime {
    fn from(lifetime: Lifetime) -> Self {
        match lifetime {
            Lifetime::Ping => Self::Ping,
            Lifetime::Application => Self::Application,
            Lifetime::User => Self::User,
        }
    }
}

impl From<MetricLifetime> for Lifetime {
    fn from(lifetime: MetricLifetime) -> Self {
        match lifetime {
            MetricLifetime::Ping => Self::Ping,
            MetricLifetime::Application => Self::Application,
            MetricLifetime::User => Self::User,
        }
    }
}
