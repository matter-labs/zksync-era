//! This module represents the conditional sealer, which can decide whether the batch
//! should be sealed after executing a particular transaction.
//!
//! The conditional sealer abstraction allows to implement different sealing strategies, e.g. the actual
//! sealing strategy for the main node or noop sealer for the external node.

use std::fmt;

use async_trait::async_trait;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_multivm::interface::TransactionExecutionMetrics;
use zksync_types::{ProtocolVersionId, Transaction};
use zksync_vm_executor::interface::TransactionFilter;

use super::{criteria, SealCriterion, SealData, SealResolution, AGGREGATION_METRICS};

/// Checks if an L1 batch should be sealed after executing a transaction.
pub trait ConditionalSealer: 'static + fmt::Debug + Send + Sync {
    /// Returns the action that should be taken by the state keeper after executing a transaction.
    #[allow(clippy::too_many_arguments)]
    fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        tx_count: usize,
        l1_tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution;

    /// Returns fractions of the criteria's capacity filled in the batch.
    fn capacity_filled(
        &self,
        tx_count: usize,
        l1_tx_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Vec<(&'static str, f64)>;
}

/// Implementation of [`ConditionalSealer`] used by the main node.
/// Internally uses a set of [`SealCriterion`]s to determine whether the batch should be sealed.
///
/// The checks are deterministic, i.e., should depend solely on execution metrics and [`StateKeeperConfig`].
/// Non-deterministic seal criteria are expressed using [`IoSealCriteria`](super::IoSealCriteria).
#[derive(Debug, Default)]
pub struct SequencerSealer {
    config: StateKeeperConfig,
    sealers: Vec<Box<dyn SealCriterion>>,
}

#[async_trait]
impl TransactionFilter for SequencerSealer {
    async fn filter_transaction(
        &self,
        transaction: &Transaction,
        metrics: &TransactionExecutionMetrics,
    ) -> Result<(), String> {
        let data = SealData::for_transaction(transaction, metrics);
        for sealer in &self.sealers {
            const TX_COUNT: usize = 1;

            let resolution = sealer.should_seal(
                &self.config,
                TX_COUNT,
                TX_COUNT,
                &data,
                &data,
                ProtocolVersionId::latest(), // FIXME: is it safe to use?
            );
            if matches!(resolution, SealResolution::Unexecutable(_)) {
                let err = sealer.prom_criterion_name().to_owned();
                return Err(err);
            }
        }
        Ok(())
    }
}

impl ConditionalSealer for SequencerSealer {
    fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        tx_count: usize,
        l1_tx_count: usize,
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
                tx_count,
                l1_tx_count,
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
                    AGGREGATION_METRICS
                        .l1_batch_reason_inc(sealer.prom_criterion_name(), &seal_resolution);
                }
                SealResolution::NoSeal => { /* Don't do anything */ }
            }

            final_seal_resolution = final_seal_resolution.stricter(seal_resolution);
        }
        final_seal_resolution
    }

    fn capacity_filled(
        &self,
        tx_count: usize,
        l1_tx_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Vec<(&'static str, f64)> {
        self.sealers
            .iter()
            .filter_map(|s| {
                let filled = s.capacity_filled(
                    &self.config,
                    tx_count,
                    l1_tx_count,
                    block_data,
                    protocol_version,
                );
                filled.map(|f| (s.prom_criterion_name(), f))
            })
            .collect()
    }
}

impl SequencerSealer {
    pub fn new(config: StateKeeperConfig) -> Self {
        let sealers = Self::default_sealers(&config);
        Self { config, sealers }
    }

    #[cfg(test)]
    pub(crate) fn with_sealers(
        config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        Self { config, sealers }
    }

    fn default_sealers(config: &StateKeeperConfig) -> Vec<Box<dyn SealCriterion>> {
        vec![
            Box::new(criteria::SlotsCriterion),
            Box::new(criteria::PubDataBytesCriterion {
                max_pubdata_per_batch: config.max_pubdata_per_batch,
            }),
            Box::new(criteria::CircuitsCriterion),
            Box::new(criteria::TxEncodingSizeCriterion),
            Box::new(criteria::GasForBatchTipCriterion),
            Box::new(criteria::L1L2TxsCriterion),
            Box::new(criteria::L2L1LogsCriterion),
        ]
    }
}

/// Implementation of [`ConditionalSealer`] that never seals the batch.
///
/// Can be used in contexts where, for example, state keeper configuration is not available,
/// or the decision to seal batch is taken by some other component.
#[derive(Debug)]
pub struct NoopSealer;

impl ConditionalSealer for NoopSealer {
    fn should_seal_l1_batch(
        &self,
        _l1_batch_number: u32,
        _tx_count: usize,
        _l1_tx_count: usize,
        _block_data: &SealData,
        _tx_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        SealResolution::NoSeal
    }

    fn capacity_filled(
        &self,
        _tx_count: usize,
        _l1_tx_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Vec<(&'static str, f64)> {
        Vec::new()
    }
}
