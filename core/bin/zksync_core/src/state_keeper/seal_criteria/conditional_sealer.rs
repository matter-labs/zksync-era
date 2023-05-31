//! This module represents the conditional sealer, which can decide whether the batch
//! should be sealed after executing a particular transaction.
//! It is used on the main node to decide when the batch should be sealed (as opposed to the external node,
//! which unconditionally follows the instructions from the main node).

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::{
    block::BlockGasCount,
    tx::{tx_execution_info::DeduplicatedWritesMetrics, ExecutionMetrics},
};

use super::{criteria, SealCriterion, SealResolution};

#[derive(Debug)]
pub struct ConditionalSealer {
    config: StateKeeperConfig,
    /// Primary sealers set that is used to check if batch should be sealed after executing a transaction.
    sealers: Vec<Box<dyn SealCriterion>>,
}

impl ConditionalSealer {
    pub(crate) fn new(config: StateKeeperConfig) -> Self {
        let sealers: Vec<Box<dyn SealCriterion>> = Self::get_default_sealers();

        Self { config, sealers }
    }

    #[cfg(test)]
    pub(crate) fn with_sealers(
        config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        Self { config, sealers }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        block_execution_metrics: ExecutionMetrics,
        tx_execution_metrics: ExecutionMetrics,
        block_gas_count: BlockGasCount,
        tx_gas_count: BlockGasCount,
        block_included_txs_size: usize,
        tx_size: usize,
        block_writes_metrics: DeduplicatedWritesMetrics,
        tx_writes_metrics: DeduplicatedWritesMetrics,
    ) -> SealResolution {
        let mut final_seal_resolution = SealResolution::NoSeal;
        for sealer in &self.sealers {
            let seal_resolution = sealer.should_seal(
                &self.config,
                block_open_timestamp_ms,
                tx_count,
                block_execution_metrics,
                tx_execution_metrics,
                block_gas_count,
                tx_gas_count,
                block_included_txs_size,
                tx_size,
                block_writes_metrics,
                tx_writes_metrics,
            );
            match seal_resolution {
                SealResolution::IncludeAndSeal => {
                    vlog::debug!(
                        "Seal block with resolution: IncludeAndSeal {} {} block: {:?}",
                        l1_batch_number,
                        sealer.prom_criterion_name(),
                        block_execution_metrics
                    );
                    metrics::counter!(
                        "server.tx_aggregation.reason",
                        1,
                        "criterion" => sealer.prom_criterion_name(),
                        "seal_resolution" => "include_and_seal",
                    );
                }
                SealResolution::ExcludeAndSeal => {
                    vlog::debug!(
                        "Seal block with resolution: ExcludeAndSeal {} {} block: {:?}",
                        l1_batch_number,
                        sealer.prom_criterion_name(),
                        block_execution_metrics
                    );
                    metrics::counter!(
                        "server.tx_aggregation.reason",
                        1,
                        "criterion" => sealer.prom_criterion_name(),
                        "seal_resolution" => "exclude_and_seal",
                    );
                }
                SealResolution::Unexecutable(_) => {
                    vlog::debug!(
                        "Unexecutable {} {} block: {:?}",
                        l1_batch_number,
                        sealer.prom_criterion_name(),
                        block_execution_metrics
                    );
                    metrics::counter!(
                        "server.tx_aggregation.reason",
                        1,
                        "criterion" => sealer.prom_criterion_name(),
                        "seal_resolution" => "unexecutable",
                    );
                }
                _ => {}
            }

            final_seal_resolution = final_seal_resolution.stricter(seal_resolution);
        }
        final_seal_resolution
    }

    pub(crate) fn get_default_sealers() -> Vec<Box<dyn SealCriterion>> {
        let sealers: Vec<Box<dyn SealCriterion>> = vec![
            Box::new(criteria::slots::SlotsCriterion),
            Box::new(criteria::gas::GasCriterion),
            Box::new(criteria::pubdata_bytes::PubDataBytesCriterion),
            Box::new(criteria::geometry_seal_criteria::InitialWritesCriterion),
            Box::new(criteria::geometry_seal_criteria::RepeatedWritesCriterion),
            Box::new(criteria::geometry_seal_criteria::MaxCyclesCriterion),
            Box::new(criteria::geometry_seal_criteria::ComputationalGasCriterion),
            Box::new(criteria::tx_encoding_size::TxEncodingSizeCriterion),
        ];
        sealers
    }
}
