//! Airbender cycle-estimator calibration features, re-exported from the upstream
//! `zksync-era-airbender-cycles-estimator` crate so they can ride in
//! [`VmExecutionStatistics`](super::VmExecutionStatistics) without pulling in
//! `multivm`.

pub use zksync_era_airbender_cycles_estimator::{
    FeatureId, FeatureVector, SAFETY_CRITICAL_FEATURES,
};

/// Batch accumulation of feature vectors — a sequencer concern, kept off the
/// VM-agnostic upstream type as an extension trait.
pub trait FeatureVectorExt {
    /// Merge every count from `other` into `self`.
    fn merge(&mut self, other: &FeatureVector);
}

impl FeatureVectorExt for FeatureVector {
    fn merge(&mut self, other: &FeatureVector) {
        for (&id, &count) in &other.counts {
            self.add(id, count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_sums_counts_across_vectors() {
        let mut a = FeatureVector::default();
        a.add(FeatureId::StorageRead, 3);
        a.add(FeatureId::FarCall, 1);
        let mut b = FeatureVector::default();
        b.add(FeatureId::StorageRead, 4);
        b.add(FeatureId::StorageWrite, 2);
        a.merge(&b);
        assert_eq!(a.get(FeatureId::StorageRead), 7);
        assert_eq!(a.get(FeatureId::FarCall), 1);
        assert_eq!(a.get(FeatureId::StorageWrite), 2);
    }
}
