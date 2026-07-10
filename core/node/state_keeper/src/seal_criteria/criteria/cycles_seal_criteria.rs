use zksync_config::configs::chain::SealCriteriaConfig;
use zksync_multivm::tracers::cycle_estimator::{
    estimate_from_features, BatchContext, CycleEstimate, FeatureId, FeatureVector,
};
use zksync_types::ProtocolVersionId;

// Local uses
use crate::seal_criteria::{SealCriterion, SealData, SealResolution, UnexecutableReason};

/// Safety margin on the raw estimate. The model is fit with an asymmetric loss and
/// leans conservative, so this only needs to cover ordinary variance; out-of-envelope
/// under-prediction is caught separately by `is_within_calibration`.
const CYCLE_ESTIMATE_MARGIN: f64 = 1.05;

/// Seals a batch when the Airbender guest cycle estimate for the work so far
/// approaches the per-proof budget (`max_cycles_per_batch`).
///
/// The estimate is linear in the accumulated feature vector, so it is computed once
/// from [`SealData::cycle_features`] rather than summed per transaction. An estimate
/// that can't be trusted (un-priced precompiles or out-of-envelope compute) is a
/// lower bound and seals the batch conservatively.
#[derive(Debug)]
pub struct CyclesCriterion;

impl CyclesCriterion {
    /// Estimate guest cycles from the traced features plus the batch-level scalars
    /// derivable from `seal_data`. Distinct storage applications stand in for the
    /// merkle leaves the tree will witness; bytecode-hashing inputs aren't available
    /// here and are omitted.
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
        if limit == 0 {
            return SealResolution::NoSeal; // disabled
        }

        let reject_bound = (limit as f64 * config.reject_tx_at_cycles_percentage).round() as u64;
        let include_and_seal_bound =
            (limit as f64 * config.close_block_at_cycles_percentage).round() as u64;

        // `tx_count` includes the tx currently being sealed.
        let tx_estimate = Self::estimate(&tx_data.cycle_features, tx_data, 1);
        let batch_estimate = Self::estimate(
            &block_data.cycle_features,
            block_data,
            tx_count.max(1) as u64,
        );

        let tx_cycles = tx_estimate.conservative(CYCLE_ESTIMATE_MARGIN);
        let batch_cycles = batch_estimate.conservative(CYCLE_ESTIMATE_MARGIN);

        // Trustworthy = all used precompiles priced and inside the calibration
        // envelope; otherwise `total` under-counts.
        let tx_trustworthy = tx_estimate.is_reliable() && tx_estimate.is_within_calibration();
        let batch_trustworthy =
            batch_estimate.is_reliable() && batch_estimate.is_within_calibration();

        // Reject only on a trusted estimate — never exclude a tx over a model gap.
        if tx_trustworthy && tx_cycles >= reject_bound {
            return UnexecutableReason::ProofWillFail.into();
        }

        // An untrustworthy estimate is a lower bound: seal rather than risk the budget.
        if !batch_trustworthy {
            tracing::warn!(
                "Batch cycle estimate is untrustworthy (un-priced: {:?}, extrapolated: {:?}); \
                 sealing conservatively",
                batch_estimate.unpriced,
                batch_estimate.extrapolated
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
        // An untrustworthy estimate is a lower bound, so its ratio would under-report.
        if !batch_estimate.is_reliable() || !batch_estimate.is_within_calibration() {
            return None;
        }
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

    /// Marginal cost of one `StorageApplication`. Driving estimates through storage
    /// keeps test batches inside the calibration envelope (arithmetic would trip the
    /// extrapolation guard).
    fn storage_per_unit() -> f64 {
        const PROBE: u64 = 1_000_000;
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::StorageApplication, PROBE);
        let raw = CyclesCriterion::estimate(&fv, &SealData::default(), 1).total;
        let per = raw.saturating_sub(model_base()) as f64 / PROBE as f64;
        assert!(per > 0.0, "StorageApplication must drive the estimate");
        per
    }

    /// An in-envelope feature vector whose raw estimate is approximately `raw_target`.
    fn features_reaching(raw_target: u64) -> FeatureVector {
        let over_base = raw_target.saturating_sub(model_base()) as f64;
        let count = (over_base / storage_per_unit()) as u64;
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::StorageApplication, count);
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
        // raw 1.85*base ⇒ conservative ~1.94*base ∈ [1.8*base close bound, 2*base limit).
        let block = block_data(features_reaching(185 * model_base() / 100));
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
        // A trusted tx whose own estimate exceeds the reject bound is unexecutable.
        let tx = block_data(features_reaching(2 * model_base()));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), SealData::default(), tx),
            UnexecutableReason::ProofWillFail.into()
        );
    }

    #[test]
    fn untrustworthy_batch_seals_conservatively() {
        // An arithmetic-dominated batch extrapolates out of the calibration envelope
        // and seals despite its raw magnitude being under the limit.
        let mut features = FeatureVector::default();
        features.add(FeatureId::RichAddressingOp, 5_000_000);
        let estimate = CyclesCriterion::estimate(&features, &SealData::default(), 1);
        assert!(
            !estimate.is_within_calibration(),
            "an arithmetic-dominated batch must extrapolate: {:?}",
            estimate.extrapolated
        );
        assert!(
            estimate.conservative(CYCLE_ESTIMATE_MARGIN)
                < config_with_limit_2x_base().max_cycles_per_batch,
            "magnitude alone must be under the limit, so the seal is due to extrapolation"
        );
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
