// SPDX-License-Identifier: Apache-2.0
//! Opt-in diagnostic timings, absent from normal builds and public C declarations.

use std::collections::BTreeMap;
use std::time::Duration;

use bethkit_core::Signature;
use serde_json::{json, Value};

/// Per-batch stage totals grouped by source record signature.
#[derive(Default)]
pub(super) struct StageProfile {
    timings: BTreeMap<String, BTreeMap<&'static str, f64>>,
}

impl StageProfile {
    /// Adds one completed stage without retaining record data.
    pub(super) fn add(&mut self, signature: Signature, stage: &'static str, elapsed: Duration) {
        *self
            .timings
            .entry(signature.to_string())
            .or_default()
            .entry(stage)
            .or_default() += elapsed.as_secs_f64();
    }

    /// Copies diagnostic seconds into an optional batch document extension.
    pub(super) fn document(&self) -> Value {
        json!(self.timings)
    }
}
