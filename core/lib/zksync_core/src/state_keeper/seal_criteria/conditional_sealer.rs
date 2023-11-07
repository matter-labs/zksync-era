//! This module represents the conditional sealer, which can decide whether the batch
//! should be sealed after executing a particular transaction.
//! It is used on the main node to decide when the batch should be sealed (as opposed to the external node,
//! which unconditionally follows the instructions from the main node).

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::ProtocolVersionId;

use super::{criteria, SealCriterion, SealData, SealResolution, AGGREGATION_METRICS};

/// Checks if an L1 batch should be sealed after executing a transaction.
///
/// The checks are deterministic, i.e., should depend solely on execution metrics and [`StateKeeperConfig`].
/// Non-deterministic seal criteria are expressed using [`IoSealCriteria`](super::IoSealCriteria).
#[derive(Debug)]
pub struct ConditionalSealer {
    config: StateKeeperConfig,
    sealers: Vec<Box<dyn SealCriterion>>,
}

impl ConditionalSealer {
    /// Finds a reason why a transaction with the specified `data` is unexecutable.
    pub(crate) fn find_unexecutable_reason(
        config: &StateKeeperConfig,
        data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Option<&'static str> {
        for sealer in &Self::default_sealers() {
            const MOCK_BLOCK_TIMESTAMP: u128 = 0;
            const TX_COUNT: usize = 1;

            let resolution = sealer.should_seal(
                config,
                MOCK_BLOCK_TIMESTAMP,
                TX_COUNT,
                data,
                data,
                protocol_version,
            );
            if matches!(resolution, SealResolution::Unexecutable(_)) {
                return Some(sealer.prom_criterion_name());
            }
        }
        None
    }

    pub(crate) fn new(config: StateKeeperConfig) -> Self {
        let sealers = Self::default_sealers();
        Self { config, sealers }
    }

    #[cfg(test)]
    pub(in crate::state_keeper) fn with_sealers(
        config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        Self { config, sealers }
    }

    pub fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        tracing::trace!(
            "Determining seal resolution for L1 batch #{l1_batch_number} with {tx_count} transactions \
             and metrics {:?}",
            block_data.execution_metrics
        );

        let mut final_seal_resolution = SealResolution::NoSeal;
        for sealer in &self.sealers {
            let seal_resolution = sealer.should_seal(
                &self.config,
                block_open_timestamp_ms,
                tx_count,
                block_data,
                tx_data,
                protocol_version,
            );
            match &seal_resolution {
                SealResolution::IncludeAndSeal
                | SealResolution::ExcludeAndSeal
                | SealResolution::Unexecutable(_) => {
                    tracing::debug!(
                        "L1 batch #{l1_batch_number} processed by `{name}` with resolution {seal_resolution:?}",
                        name = sealer.prom_criterion_name()
                    );
                    AGGREGATION_METRICS.inc(sealer.prom_criterion_name(), &seal_resolution);
                }
                SealResolution::NoSeal => { /* Don't do anything */ }
            }

            final_seal_resolution = final_seal_resolution.stricter(seal_resolution);
        }
        final_seal_resolution
    }

    fn default_sealers() -> Vec<Box<dyn SealCriterion>> {
        vec![
            Box::new(criteria::SlotsCriterion),
            Box::new(criteria::GasCriterion),
            Box::new(criteria::PubDataBytesCriterion),
            Box::new(criteria::InitialWritesCriterion),
            Box::new(criteria::RepeatedWritesCriterion),
            Box::new(criteria::MaxCyclesCriterion),
            Box::new(criteria::ComputationalGasCriterion),
            Box::new(criteria::TxEncodingSizeCriterion),
            Box::new(criteria::L2ToL1LogsCriterion),
        ]
    }
}
