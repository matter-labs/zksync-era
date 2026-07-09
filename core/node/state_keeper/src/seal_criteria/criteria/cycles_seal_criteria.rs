use zksync_config::configs::chain::SealCriteriaConfig;
use zksync_multivm::tracers::cycle_estimator::{
    estimate_from_features, BatchContext, CycleEstimate, FeatureId, FeatureVector,
};
use zksync_types::ProtocolVersionId;

// Local uses
use crate::seal_criteria::{SealCriterion, SealData, SealResolution, UnexecutableReason};

/// Safety margin applied to a raw cycle estimate before comparing it to the limit.
/// The calibrated model systematically under-predicts by a couple of percent; this
/// absorbs ordinary variance. It does NOT rescue an *unreliable* estimate (one that
/// omits an un-priced precompile) — that is handled separately (fail-safe seal).
const CYCLE_ESTIMATE_MARGIN: f64 = 1.10;

/// Seals a batch once the Airbender guest cycle-count estimate for the work executed
/// so far approaches the per-proof budget (`max_cycles_per_batch`).
///
/// Unlike [`CircuitsCriterion`](super::CircuitsCriterion), which reads a scalar that
/// is additive across transactions, the cycle estimate is a *linear* function of the
/// accumulated feature vector (`total = base + Σ coeff·feature`), so the estimate is
/// computed here from [`SealData::cycle_features`] rather than summed per transaction.
///
/// Fail-safe behavior: if a batch uses a safety-critical precompile the model does
/// not price, the estimate is a lower bound and cannot be trusted. In that case the
/// criterion seals the batch (`IncludeAndSeal`) rather than risk exceeding the proof
/// budget with unaccounted work.
#[derive(Debug)]
pub struct CyclesCriterion;

impl CyclesCriterion {
    /// Estimate guest cycles for the work described by `features` + the batch-level
    /// scalars derivable from `seal_data`.
    ///
    /// At sequencing time the merkle witness does not exist yet, so the number of
    /// distinct storage applications is used as the estimate of the leaves the tree
    /// will witness (as in the estimator's own tests). Bytecode-hashing inputs are
    /// not available from `SealData` and are treated as zero; the safety margin and
    /// the `close_block` percentage provide headroom for this approximation.
    fn estimate(
        features: &FeatureVector,
        seal_data: &SealData,
        transaction_count: u64,
    ) -> CycleEstimate {
        let storage_applications = features.get(FeatureId::StorageApplication);
        let ctx = BatchContext {
            transaction_count,
            merkle_leaf_count: storage_applications,
            storage_key_count: storage_applications,
            used_bytecode_bytes: 0,
            used_bytecode_count: 0,
        };
        let pubdata_bytes = u64::from(seal_data.execution_metrics.pubdata_published);
        let state_diff_count = (seal_data.writes_metrics.initial_storage_writes
            + seal_data.writes_metrics.repeated_storage_writes)
            as u64;
        estimate_from_features(features.clone(), pubdata_bytes, state_diff_count, &ctx)
    }
}

impl SealCriterion for CyclesCriterion {
    fn should_seal(
        &self,
        config: &SealCriteriaConfig,
        tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        let limit = config.max_cycles_per_batch;
        // A zero limit is degenerate; treat it as "cycle sealing disabled".
        if limit == 0 {
            return SealResolution::NoSeal;
        }

        let reject_bound = (limit as f64 * config.reject_tx_at_cycles_percentage).round() as u64;
        let include_and_seal_bound =
            (limit as f64 * config.close_block_at_cycles_percentage).round() as u64;

        // `tx_count` counts transactions *including* the one currently being sealed.
        let tx_estimate = Self::estimate(&tx_data.cycle_features, tx_data, 1);
        let batch_estimate = Self::estimate(
            &block_data.cycle_features,
            block_data,
            tx_count.max(1) as u64,
        );

        let tx_cycles = tx_estimate.conservative(CYCLE_ESTIMATE_MARGIN);
        let batch_cycles = batch_estimate.conservative(CYCLE_ESTIMATE_MARGIN);

        // Only reject a single transaction outright when we *trust* its estimate.
        // Rejecting on an unreliable (under-counted) estimate could permanently
        // exclude an otherwise-valid transaction merely because the model has a gap.
        if tx_estimate.is_reliable() && tx_cycles >= reject_bound {
            return UnexecutableReason::ProofWillFail.into();
        }

        // Fail safe: an unreliable batch estimate omits un-priced precompile work, so
        // `batch_cycles` is a lower bound. Seal now rather than risk overflowing the
        // proof budget with work the model can't see.
        if !batch_estimate.is_reliable() {
            tracing::warn!(
                "Batch cycle estimate is unreliable (un-priced precompiles used: {:?}); \
                 sealing conservatively",
                batch_estimate.unpriced
            );
            return SealResolution::IncludeAndSeal;
        }

        if batch_cycles >= limit {
            SealResolution::ExcludeAndSeal
        } else if batch_cycles >= include_and_seal_bound {
            SealResolution::IncludeAndSeal
        } else {
            SealResolution::NoSeal
        }
    }

    fn capacity_filled(
        &self,
        config: &SealCriteriaConfig,
        tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        if config.max_cycles_per_batch == 0 {
            return None;
        }
        let batch_estimate = Self::estimate(
            &block_data.cycle_features,
            block_data,
            tx_count.max(1) as u64,
        );
        let used = batch_estimate.conservative(CYCLE_ESTIMATE_MARGIN) as f64;
        let full = config.max_cycles_per_batch as f64;
        Some(used / full)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "cycles_criterion"
    }
}

#[cfg(test)]
mod tests {
    use zksync_multivm::interface::FeatureVector;

    use super::*;

    /// The embedded cost model's fixed per-batch base cost (an empty batch's estimate).
    /// Computed at runtime so the tests stay valid across model recalibrations.
    fn model_base() -> u64 {
        CyclesCriterion::estimate(&FeatureVector::default(), &SealData::default(), 1).total
    }

    /// Per-`RichAddressingOp` marginal cycle cost under the embedded model.
    fn rich_per_op() -> f64 {
        const PROBE: u64 = 1_000_000;
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::RichAddressingOp, PROBE);
        let raw = CyclesCriterion::estimate(&fv, &SealData::default(), 1).total;
        let per_op = raw.saturating_sub(model_base()) as f64 / PROBE as f64;
        assert!(per_op > 0.0, "RichAddressingOp must be priced by the model");
        per_op
    }

    /// A feature vector whose raw estimate is approximately `raw_target` cycles.
    fn features_reaching(raw_target: u64) -> FeatureVector {
        let over_base = raw_target.saturating_sub(model_base()) as f64;
        let count = (over_base / rich_per_op()) as u64;
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::RichAddressingOp, count);
        fv
    }

    /// Config whose limit is twice the model base, so the whole `[base, limit]` band
    /// is reachable and thresholds land at predictable multiples of the base.
    fn config_with_limit_2x_base() -> SealCriteriaConfig {
        SealCriteriaConfig {
            max_cycles_per_batch: 2 * model_base(),
            reject_tx_at_cycles_percentage: 0.9,
            close_block_at_cycles_percentage: 0.9,
            ..SealCriteriaConfig::for_tests()
        }
    }

    fn block_data(features: FeatureVector) -> SealData {
        SealData {
            cycle_features: features,
            ..SealData::default()
        }
    }

    fn should_seal(config: &SealCriteriaConfig, block: SealData, tx: SealData) -> SealResolution {
        CyclesCriterion.should_seal(config, 1, 0, 0, &block, &tx, ProtocolVersionId::latest())
    }

    #[test]
    fn no_seal_when_well_under_limit() {
        // raw 1.5*base ⇒ conservative 1.65*base < close bound 1.8*base.
        let block = block_data(features_reaching(3 * model_base() / 2));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), block, SealData::default()),
            SealResolution::NoSeal
        );
    }

    #[test]
    fn include_and_seal_when_over_close_bound() {
        // raw 1.7*base ⇒ conservative 1.87*base ∈ [1.8*base close bound, 2*base limit).
        let block = block_data(features_reaching(17 * model_base() / 10));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), block, SealData::default()),
            SealResolution::IncludeAndSeal
        );
    }

    #[test]
    fn exclude_and_seal_when_over_limit() {
        // raw 2*base ⇒ conservative 2.2*base ≥ limit 2*base.
        let block = block_data(features_reaching(2 * model_base()));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), block, SealData::default()),
            SealResolution::ExcludeAndSeal
        );
    }

    #[test]
    fn single_oversized_tx_is_rejected() {
        // A single reliable transaction whose own conservative estimate exceeds the
        // reject bound is unexecutable.
        let tx = block_data(features_reaching(2 * model_base()));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), SealData::default(), tx),
            UnexecutableReason::ProofWillFail.into()
        );
    }

    #[test]
    fn unreliable_batch_seals_conservatively() {
        // An un-priced safety-critical precompile makes the estimate unreliable even
        // though the priced work is tiny — seal rather than trust an under-count.
        let mut features = FeatureVector::default();
        features.add(FeatureId::EcPairingCycles, 1);
        assert_eq!(
            should_seal(
                &config_with_limit_2x_base(),
                block_data(features),
                SealData::default()
            ),
            SealResolution::IncludeAndSeal
        );
    }

    #[test]
    fn disabled_when_limit_is_zero() {
        let config = SealCriteriaConfig {
            max_cycles_per_batch: 0,
            ..config_with_limit_2x_base()
        };
        // Even a hugely over-budget batch does not seal when the criterion is disabled.
        let block = block_data(features_reaching(10 * model_base()));
        assert_eq!(
            should_seal(&config, block, SealData::default()),
            SealResolution::NoSeal
        );
    }
}
