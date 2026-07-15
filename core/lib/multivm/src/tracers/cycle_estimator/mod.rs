//! Airbender guest cycle-count estimator tracer for the legacy (non-fast) VM.
//!
//! The feature schema, calibrated cost model and pure estimator are the single
//! source of truth in the upstream `zksync-era-airbender-cycles-estimator` crate
//! (shared with the fast-VM tracer) and re-exported below; this module owns only
//! the legacy-VM tracer that fills the [`FeatureVector`]. Implemented for
//! `vm_latest` only, like [`CallTracer`](crate::tracers::CallTracer).

use std::sync::Arc;

use once_cell::sync::OnceCell;

pub mod vm_latest;

pub use zksync_era_airbender_cycles_estimator::{
    estimate_from_features, BatchContext, CostModel, CycleEstimate, FeatureId, FeatureVector,
};

/// Observe-only tracer that counts the calibration features an Airbender cycle
/// estimate needs from a legacy-VM execution. Never mutates VM state, so a batch
/// runs identically with or without it.
#[derive(Debug, Clone)]
pub struct CycleFeatureTracer {
    features: FeatureVector,

    // Cursors into the VM history logs so `finish_cycle` only processes new entries.
    last_decommitment_history_entry_checked: Option<usize>,
    last_written_keys_history_entry_checked: Option<usize>,
    last_read_keys_history_entry_checked: Option<usize>,
    last_precompile_inner_entry_checked: Option<usize>,

    /// Snapshot published after the run for handle-only callers (e.g. via the tracer
    /// dispatcher), like [`CallTracer`](crate::tracers::CallTracer).
    result: Arc<OnceCell<FeatureVector>>,
}

impl Default for CycleFeatureTracer {
    /// For built-in use, where features are read back via [`Self::snapshot`] rather
    /// than through the shared `result` handle.
    fn default() -> Self {
        Self::new(Arc::new(OnceCell::new()))
    }
}

impl CycleFeatureTracer {
    /// Create a tracer that publishes its final [`FeatureVector`] into `result`.
    pub fn new(result: Arc<OnceCell<FeatureVector>>) -> Self {
        Self {
            features: FeatureVector::default(),
            last_decommitment_history_entry_checked: None,
            last_written_keys_history_entry_checked: None,
            last_read_keys_history_entry_checked: None,
            last_precompile_inner_entry_checked: None,
            result,
        }
    }

    /// Snapshot the features accumulated so far.
    pub fn snapshot(&self) -> FeatureVector {
        self.features.clone()
    }

    fn bump(&mut self, id: FeatureId, n: u64) {
        self.features.add(id, n);
    }

    /// Estimate cycles from the traced features plus the batch-level scalars the
    /// trace cannot observe (`pubdata_bytes`/`state_diff_count` from the finished
    /// batch, the rest from [`BatchContext`]).
    pub fn estimate(
        &self,
        pubdata_bytes: u64,
        state_diff_count: u64,
        ctx: &BatchContext,
    ) -> CycleEstimate {
        estimate_from_features(self.snapshot(), pubdata_bytes, state_diff_count, ctx)
    }

    /// The embedded calibrated cost model (shared with the fast VM).
    pub fn cost_model() -> &'static CostModel {
        CostModel::embedded()
    }
}
