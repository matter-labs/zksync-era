//! Airbender guest cycle-count estimator tracer for the legacy (non-fast) VM.
//!
//! The feature schema, cost table and estimator live in the
//! `zksync-era-airbender-cycles-estimator` crate and are re-exported below; this module
//! owns the legacy-VM tracer that fills the [`FeatureVector`]. Implemented for
//! `vm_latest` only, like [`CallTracer`](crate::tracers::CallTracer).
//!
//! The opcode → feature mapping must match the fast-VM tracer and the crate's vm2
//! producer: a feature no tracer fills is indistinguishable from work the batch never
//! did, so a mis-mapping under-prices in silence.

use std::sync::Arc;

use once_cell::sync::OnceCell;

pub mod vm_latest;

pub use zksync_era_airbender_cycles_estimator::{
    estimate_from_features, CostTable, CycleEstimate, FeatureId, FeatureVector,
    PROVING_CYCLE_CEILING,
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

    /// Set when the instruction currently executing is a DECOMMIT opcode; read back in
    /// `finish_cycle`, which runs once per instruction, to tell a repeat decommit (no
    /// fresh decommit recorded during this cycle) from a fresh one.
    decommit_opcode_pending: bool,

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
            decommit_opcode_pending: false,
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
    /// trace cannot observe.
    pub fn estimate(
        &self,
        pubdata_bytes: u64,
        state_diff_count: u64,
        transaction_count: u64,
        merkle_leaf_count: u64,
    ) -> CycleEstimate {
        estimate_from_features(
            self.snapshot(),
            pubdata_bytes,
            state_diff_count,
            transaction_count,
            merkle_leaf_count,
        )
    }

    /// The embedded measured cost table (shared with the fast VM).
    pub fn cost_table() -> &'static CostTable {
        CostTable::embedded()
    }
}
