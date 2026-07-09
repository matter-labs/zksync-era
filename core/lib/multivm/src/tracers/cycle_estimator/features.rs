//! Vendored from the `zksync-era-airbender-cycles-estimator` crate
//! (matter-labs/eravm-airbender-verifier). Kept in-tree so the Airbender
//! cycle-estimator tracer is self-contained in zksync-era.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Stable, ordered identifier for one calibration feature (a model INPUT —
/// something the sequencer can compute natively from a VM trace).
///
/// Opcode-family variants mirror the buckets in the VM circuit tracers
/// (`vm_fast::tracers::circuits` / `vm_latest::tracers::circuits_tracer`);
/// crypto/size variants add Airbender-relevant dimensions those tracers omit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum FeatureId {
    // vm opcode-family counts
    RichAddressingOp,
    AverageOp,
    StorageRead,
    StorageWrite,
    TransientStorageRead,
    TransientStorageWrite,
    Event,
    PrecompileCall,
    Decommit,
    FarCall,
    UmaWrite,
    UmaRead,
    // crypto complexity (value = cycles/rounds)
    Keccak256Cycles,
    Sha256Cycles,
    EcRecoverCycles,
    Secp256r1VerifyCycles,
    ModExpCycles,
    EcAddCycles,
    EcMulCycles,
    EcPairingCycles,
    DecommitCycles,
    StorageApplication,
    // Airbender size features
    HeapGrowthBytes,
    CopyBytes,
    DecommitBytes,
    // batch-level features
    TransactionCount,
    NearCallCount,
    PubdataBytes,
    MerkleLeafCount,
    // setup-phase drivers (batch-level, from input.vm_run_data) — the verifier
    // hashes every used bytecode and materializes the storage view + initial
    // heap before the VM runs; the vm tracer never observes this work.
    UsedBytecodeBytes,
    UsedBytecodeCount,
    StorageKeyCount,
    InitialHeapWords,
    // commitment-phase drivers (from the finished batch) — keccak over the
    // serialized state diffs and system logs.
    StateDiffCount,
    SystemLogCount,
}

/// Precompile / crypto complexity features that are expensive per unit and whose
/// cost the model MUST price for an estimate to be trustworthy. If a batch uses
/// one of these but the model prices it at ~0 (no coefficient — e.g. the corpus
/// never exercised that precompile), the prediction silently omits that work and
/// is an under-estimate. The estimator flags this via
/// [`crate::tracers::cycle_estimator::model::CostModel::unpriced_used`] so callers
/// can fail safe.
///
/// Each is measured as operation complexity (hashing rounds / circuit cycles),
/// so it already scales with input size — a bigger keccak yields a bigger count.
pub const SAFETY_CRITICAL_FEATURES: &[FeatureId] = &[
    FeatureId::Keccak256Cycles,
    FeatureId::Sha256Cycles,
    FeatureId::EcRecoverCycles,
    FeatureId::Secp256r1VerifyCycles,
    FeatureId::ModExpCycles,
    FeatureId::EcAddCycles,
    FeatureId::EcMulCycles,
    FeatureId::EcPairingCycles,
];

/// A calibration feature vector: model INPUTS only (no measured cycles).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureVector {
    pub counts: BTreeMap<FeatureId, u64>,
}

impl FeatureVector {
    /// Accumulate `n` occurrences of `id`.
    pub fn add(&mut self, id: FeatureId, n: u64) {
        *self.counts.entry(id).or_insert(0) += n;
    }

    /// Current count for `id` (0 if never added).
    pub fn get(&self, id: FeatureId) -> u64 {
        self.counts.get(&id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_accumulates_and_get_defaults_zero() {
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::StorageRead, 3);
        fv.add(FeatureId::StorageRead, 4);
        assert_eq!(fv.get(FeatureId::StorageRead), 7);
        assert_eq!(fv.get(FeatureId::FarCall), 0);
    }

    #[test]
    fn json_roundtrip_is_stable() {
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::Keccak256Cycles, 42);
        let json = serde_json::to_string(&fv).unwrap();
        let back: FeatureVector = serde_json::from_str(&json).unwrap();
        assert_eq!(fv, back);
        assert!(json.contains("keccak256_cycles"));
    }
}
