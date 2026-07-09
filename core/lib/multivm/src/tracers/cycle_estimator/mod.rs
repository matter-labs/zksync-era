//! Airbender guest cycle-count estimator tracer for the legacy (non-fast) VM.
//!
//! This is the `zksync-era` sibling of the fast-VM (`zksync_vm2`) tracer in
//! matter-labs/eravm-airbender-verifier. The calibrated cost model and feature
//! schema are vendored here ([`features`], [`model`], [`estimator`], plus the
//! embedded `cost_table.json`) so the tracer is fully self-contained in era.
//!
//! The tracer fills the same [`FeatureVector`] — opcode-family counts plus
//! precompile/decommit/storage complexity — so it feeds the **same calibrated
//! cost model** ([`CostModel::embedded`]) the fast VM uses. Only the observation
//! mechanism differs: the legacy VM is observed by bucketing opcodes in
//! `before_execution` (mirroring [`crate::vm_latest::tracers::circuits_tracer`])
//! and by scanning the VM's decommit / storage / precompile history logs each
//! `finish_cycle`.
//!
//! Implemented for the latest available VM only (`vm_latest`), analogous to how
//! [`CallTracer`](crate::tracers::CallTracer) is wired per VM version.

use std::sync::Arc;

use once_cell::sync::OnceCell;

pub mod estimator;
pub mod features;
pub mod model;
pub mod vm_latest;

pub use estimator::{
    estimate_from_features, estimate_from_features_with_model, features_for_estimate_from,
    BatchContext, CycleEstimate,
};
pub use features::{FeatureId, FeatureVector, SAFETY_CRITICAL_FEATURES};
pub use model::{CostModel, LinearModel};

/// Passive tracer that counts the calibration features an Airbender cycle-cost
/// estimate needs, from a legacy-VM execution.
///
/// It only observes — every hook accumulates counts and never mutates VM state —
/// so a batch executed with this tracer runs identically to one without it. The
/// per-feature cycle weights live in the calibrated cost model, never here.
#[derive(Debug, Clone)]
pub struct CycleFeatureTracer {
    /// Accumulated model-input features (opcode families + crypto/decommit/storage
    /// complexity) observed so far. Batch-level features (pubdata, merkle leaves,
    /// bytecodes, …) are added later in [`Self::estimate`] from caller-supplied
    /// scalars, since the VM trace cannot observe them.
    features: FeatureVector,

    // Cursors into the VM's history logs so `finish_cycle` only processes entries
    // added since the previous cycle. Set in `initialize_tracer`.
    last_decommitment_history_entry_checked: Option<usize>,
    last_written_keys_history_entry_checked: Option<usize>,
    last_read_keys_history_entry_checked: Option<usize>,
    last_precompile_inner_entry_checked: Option<usize>,

    /// Published snapshot of `features`, set after the run — for callers that hand
    /// the tracer off (e.g. through the tracer dispatcher) and only keep a handle,
    /// exactly as [`CallTracer`](crate::tracers::CallTracer) exposes its result.
    result: Arc<OnceCell<FeatureVector>>,
}

impl CycleFeatureTracer {
    /// Create a tracer that publishes its final [`FeatureVector`] into `result`
    /// once the VM finishes executing.
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

    /// Combine the traced vm-execution features with the batch-level scalars the
    /// trace cannot observe and the embedded calibrated cost model into a cycle
    /// estimate. `pubdata_bytes` and `state_diff_count` come from the finished
    /// batch; the rest from [`BatchContext`].
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
