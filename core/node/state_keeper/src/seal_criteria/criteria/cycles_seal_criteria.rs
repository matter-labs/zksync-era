use zksync_config::configs::chain::SealCriteriaConfig;
use zksync_multivm::tracers::cycle_estimator::{
    estimate_from_features, CycleEstimate, FeatureId, FeatureVector,
};
use zksync_types::ProtocolVersionId;

// Local uses
use crate::seal_criteria::{SealCriterion, SealData, SealResolution, UnexecutableReason};

/// Seals a batch when the Airbender guest cycle estimate for the work so far approaches
/// the per-proof budget (`max_cycles_per_batch`).
///
/// The estimate is linear in the accumulated feature vector, so it is computed once from
/// [`SealData::cycle_features`] rather than summed per transaction.
///
/// `max_cycles_per_batch` is the only seal condition, used as configured; the config
/// bounds it by Airbender's proving ceiling, so staying inside the budget also keeps a
/// sealed batch provable. An estimate the model cannot stand behind is reported by the
/// seal logic and suppresses a rejection here, but never seals.
#[derive(Debug)]
pub struct CyclesCriterion;

/// Estimate guest cycles from traced features plus batch-level scalars. Distinct
/// storage applications stand in for the merkle leaves the tree will witness, which
/// do not exist yet at sealing time.
///
/// Shared between [`CyclesCriterion`] and the L1 batch seal logic, which persists
/// the prediction for later comparison against the prover-reported cycle count.
pub(crate) fn estimate_batch_cycles(
    features: &FeatureVector,
    pubdata_bytes: u64,
    state_diff_count: u64,
    transaction_count: u64,
) -> CycleEstimate {
    let merkle_leaf_count = features.get(FeatureId::StorageApplication);
    estimate_from_features(
        features.clone(),
        pubdata_bytes,
        state_diff_count,
        transaction_count,
        merkle_leaf_count,
    )
}

impl CyclesCriterion {
    /// [`estimate_batch_cycles`] with the batch-level scalars derived from `seal_data`.
    fn estimate(
        features: &FeatureVector,
        seal_data: &SealData,
        transaction_count: u64,
    ) -> CycleEstimate {
        let pubdata_bytes = u64::from(seal_data.execution_metrics.pubdata_published);
        let state_diff_count = (seal_data.writes_metrics.initial_storage_writes
            + seal_data.writes_metrics.repeated_storage_writes)
            as u64;
        estimate_batch_cycles(features, pubdata_bytes, state_diff_count, transaction_count)
    }

    /// A single transaction is priced without the per-batch base, which belongs to the
    /// batch and not to any one transaction in it.
    fn conservative_marginal(estimate: &CycleEstimate) -> u64 {
        (estimate.marginal as f64 * estimate.margin.max(1.0)).ceil() as u64
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
        let is_tx_first = tx_count == 1;
        let tx_estimate = Self::estimate(&tx_data.cycle_features, tx_data, 1);
        let batch_estimate = Self::estimate(
            &block_data.cycle_features,
            block_data,
            tx_count.max(1) as u64,
        );

        let tx_cycles = Self::conservative_marginal(&tx_estimate);
        let batch_cycles = batch_estimate.conservative();

        // Trusted = every used operation priced, every count inside the domain its rate
        // was measured over, and a producer that filled the axes it should have.
        let tx_trustworthy = tx_estimate.is_reliable() && tx_estimate.is_within_calibration();

        // Reject on the transaction's own cost only when that cost can be trusted — a
        // model gap is not the sender's fault.
        if tx_trustworthy && tx_cycles >= reject_bound {
            // A bounded rate prices the worst operand shape the tracer cannot see, so a
            // refusal resting on one may be far above the transaction's real cost.
            tracing::warn!(
                "Rejecting transaction at {tx_cycles} cycles (bound {reject_bound}); \
                 {:.0}% of the estimate comes from bounded rates",
                tx_estimate.bounded_share() * 100.0
            );
            return UnexecutableReason::ProofWillFail.into();
        }

        if batch_cycles >= limit {
            // Excluding the only transaction seals an empty batch and hands that
            // transaction straight back, forever; an L1 or upgrade transaction there halts
            // the chain. Refuse it instead, trusted or not — this bound includes the
            // per-batch base and the reject bound above does not, so a transaction can
            // clear that one and still land here.
            if is_tx_first {
                UnexecutableReason::ProofWillFail.into()
            } else {
                SealResolution::ExcludeAndSeal
            }
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
        let limit = config.max_cycles_per_batch;
        if limit == 0 {
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
        Some(batch_estimate.conservative() as f64 / limit as f64)
    }

    fn prom_criterion_name(&self) -> &'static str {
        "cycles_criterion"
    }
}

#[cfg(test)]
mod tests {
    use zksync_multivm::{
        interface::FeatureVector, tracers::cycle_estimator::PROVING_CYCLE_CEILING,
    };

    use super::*;

    /// The embedded cost table's fixed per-batch base cost (an empty batch's estimate).
    /// Computed at runtime so the tests stay valid across model recalibrations.
    fn model_base() -> u64 {
        CyclesCriterion::estimate(&FeatureVector::default(), &SealData::default(), 1).total
    }

    /// Marginal cost of one far call. Far calls are a traced feature, so driving
    /// estimates through them keeps batches out of the missing-trace signal, and the
    /// counts below stay inside the rate's calibrated domain.
    fn far_call_cost() -> f64 {
        const PROBE: u64 = 1_000;
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::FarCall, PROBE);
        let raw = CyclesCriterion::estimate(&fv, &SealData::default(), 1).total;
        let per = raw.saturating_sub(model_base()) as f64 / PROBE as f64;
        assert!(per > 0.0, "FarCall must drive the estimate");
        per
    }

    /// An in-domain feature vector whose raw estimate is approximately `raw_target`.
    fn features_reaching(raw_target: u64) -> FeatureVector {
        let over_base = raw_target.saturating_sub(model_base()) as f64;
        let mut fv = FeatureVector::default();
        fv.add(FeatureId::FarCall, (over_base / far_call_cost()) as u64);
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

    /// `tx_count` counts the transaction being sealed, so anything above 1 means the batch
    /// already holds one that a seal would keep.
    fn should_seal_nth(
        config: &SealCriteriaConfig,
        tx_count: usize,
        block: SealData,
        tx: SealData,
    ) -> SealResolution {
        CyclesCriterion.should_seal(
            config,
            tx_count,
            0,
            0,
            &block,
            &tx,
            ProtocolVersionId::latest(),
        )
    }

    #[test]
    fn no_seal_when_well_under_limit() {
        // raw 1.3*base ⇒ conservative 1.69*base < close bound 1.8*base.
        let block = block_data(features_reaching(13 * model_base() / 10));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), block, SealData::default()),
            SealResolution::NoSeal
        );
    }

    #[test]
    fn include_and_seal_when_over_close_bound() {
        // raw 1.45*base ⇒ conservative ~1.89*base ∈ [1.8*base close bound, 2*base limit).
        let block = block_data(features_reaching(145 * model_base() / 100));
        assert_eq!(
            should_seal(&config_with_limit_2x_base(), block, SealData::default()),
            SealResolution::IncludeAndSeal
        );
    }

    #[test]
    fn exclude_and_seal_when_over_limit() {
        // raw 1.6*base ⇒ conservative 2.08*base ≥ limit 2*base. Sealed as the second
        // transaction, so excluding it leaves a batch with something in it.
        let block = block_data(features_reaching(16 * model_base() / 10));
        assert_eq!(
            should_seal_nth(&config_with_limit_2x_base(), 2, block, SealData::default()),
            SealResolution::ExcludeAndSeal
        );
    }

    #[test]
    fn single_oversized_tx_is_rejected() {
        // Own cost 0.8*base ⇒ conservative 1.04*base ≥ the 1.0*base reject bound.
        let config = SealCriteriaConfig {
            reject_tx_at_cycles_percentage: 0.5,
            ..config_with_limit_2x_base()
        };
        let tx = block_data(features_reaching(18 * model_base() / 10));
        assert_eq!(
            should_seal(&config, SealData::default(), tx),
            UnexecutableReason::ProofWillFail.into()
        );
    }

    /// A transaction is priced without the per-batch base: own cost 0.5*base ⇒
    /// conservative 0.65*base, under the 1.8*base reject bound that the same estimate
    /// including the base (1.95*base) would cross.
    #[test]
    fn a_tx_is_not_rejected_for_the_batch_base() {
        let features = features_reaching(15 * model_base() / 10);
        assert_ne!(
            should_seal(
                &config_with_limit_2x_base(),
                block_data(features.clone()),
                block_data(features)
            ),
            UnexecutableReason::ProofWillFail.into()
        );
    }

    /// An estimate the model cannot stand behind does not seal — only the budget does.
    /// Two shapes reach it: a count outside the range its rate was measured over, and a
    /// producer that folded the five arithmetic classes into the cheap one.
    #[test]
    fn an_untrustworthy_estimate_does_not_seal() {
        let mut out_of_domain = FeatureVector::default();
        out_of_domain.add(FeatureId::Event, 100_000);
        let mut folded_arithmetic = FeatureVector::default();
        folded_arithmetic.add(FeatureId::ArithCheapOp, 2_000_000);

        for features in [out_of_domain, folded_arithmetic] {
            let estimate = CyclesCriterion::estimate(&features, &SealData::default(), 1);
            assert!(
                !estimate.is_reliable() || !estimate.is_within_calibration(),
                "{estimate:?}"
            );
            assert!(
                estimate.conservative() < config_with_limit_2x_base().max_cycles_per_batch,
                "magnitude alone must stay under the limit: {estimate:?}"
            );
            assert_eq!(
                should_seal(
                    &config_with_limit_2x_base(),
                    block_data(features),
                    SealData::default()
                ),
                SealResolution::NoSeal
            );
        }
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

    /// The largest budget the config accepts is the proving ceiling, so a batch predicted
    /// to be unprovable is over budget by construction and seals.
    #[test]
    fn a_batch_past_the_proving_ceiling_seals_under_the_largest_budget() {
        let config = SealCriteriaConfig {
            max_cycles_per_batch: PROVING_CYCLE_CEILING,
            ..SealCriteriaConfig::for_tests()
        };
        let block = block_data(features_reaching(2 * PROVING_CYCLE_CEILING));
        assert!(
            CyclesCriterion::estimate(&block.cycle_features, &block, 1).conservative()
                > PROVING_CYCLE_CEILING
        );
        assert_eq!(
            should_seal_nth(&config, 2, block, SealData::default()),
            SealResolution::ExcludeAndSeal
        );
    }

    /// A batch over the budget seals by excluding its last transaction, except when that
    /// transaction is the only one in it: exclusion would seal an empty batch and put the
    /// same transaction at the head of the next one, forever.
    #[test]
    fn a_lone_over_budget_tx_is_refused_not_excluded() {
        let features = features_reaching(3 * model_base());
        assert_eq!(
            should_seal(
                &config_with_limit_2x_base(),
                block_data(features.clone()),
                block_data(features)
            ),
            UnexecutableReason::ProofWillFail.into()
        );
    }

    /// Distrust suppresses the reject bound, so refusal here has to come from the batch
    /// branch.
    #[test]
    fn an_untrusted_lone_over_budget_tx_is_refused_too() {
        let mut features = FeatureVector::default();
        features.add(FeatureId::FarCall, 200_000);
        let estimate = CyclesCriterion::estimate(&features, &SealData::default(), 1);
        assert!(
            !estimate.is_within_calibration(),
            "the driver must be out of its calibrated domain for this to test distrust"
        );
        assert!(
            estimate.conservative() >= config_with_limit_2x_base().max_cycles_per_batch,
            "and it must still be over the budget"
        );
        assert_eq!(
            should_seal(
                &config_with_limit_2x_base(),
                block_data(features.clone()),
                block_data(features)
            ),
            UnexecutableReason::ProofWillFail.into()
        );
    }

    /// With a transaction already in the batch, exclusion makes progress.
    #[test]
    fn an_over_budget_batch_with_earlier_txs_still_excludes() {
        let features = features_reaching(3 * model_base());
        assert_eq!(
            should_seal_nth(
                &config_with_limit_2x_base(),
                2,
                block_data(features.clone()),
                block_data(features)
            ),
            SealResolution::ExcludeAndSeal
        );
    }
}
