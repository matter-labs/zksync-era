//! This module represents the conditional sealer, which can decide whether the batch
//! should be sealed after executing a particular transaction.
//!
//! The conditional sealer abstraction allows to implement different sealing strategies, e.g. the actual
//! sealing strategy for the main node or noop sealer for the external node.

use std::fmt;

use async_trait::async_trait;
use zksync_config::configs::chain::SealCriteriaConfig;
use zksync_multivm::{
    interface::TransactionExecutionMetrics,
    utils::{
        get_bootloader_max_txs_in_batch, get_max_batch_base_layer_circuits,
        get_max_vm_pubdata_per_batch,
    },
};
use zksync_types::{commitment::L1BatchCommitmentMode, ProtocolVersionId, Transaction};
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
        interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution;

    /// Returns fractions of the criteria's capacity filled in the batch.
    fn capacity_filled(
        &self,
        tx_count: usize,
        l1_tx_count: usize,
        interop_roots_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Vec<(&'static str, f64)>;
}

/// Implementation of [`ConditionalSealer`] used by the main node.
/// Internally uses a set of [`SealCriterion`]s to determine whether the batch should be sealed.
///
/// The checks are deterministic, i.e., should depend solely on execution metrics and [`StateKeeperConfig`].
/// Non-deterministic seal criteria are expressed using [`IoSealCriteria`](super::IoSealCriteria).
#[derive(Debug)]
pub struct SequencerSealer {
    config: SealCriteriaConfig,
    sealers: Vec<Box<dyn SealCriterion>>,
}

#[cfg(test)]
impl SequencerSealer {
    pub(crate) fn for_tests() -> Self {
        Self {
            config: SealCriteriaConfig::for_tests(),
            sealers: vec![],
        }
    }
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
                TX_COUNT,
                &data,
                &data,
                ProtocolVersionId::latest(),
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
        interop_roots_count: usize,
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
                interop_roots_count,
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
        interop_roots_count: usize,
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
                    interop_roots_count,
                    block_data,
                    protocol_version,
                );
                filled.map(|f| (s.prom_criterion_name(), f))
            })
            .collect()
    }
}

/// Sealers excluding pubdata for verifying blocks produced by sequencer
/// on validium chains through PanicSealer
fn validium_sealers() -> Vec<Box<dyn SealCriterion>> {
    vec![
        Box::new(criteria::SlotsCriterion),
        Box::new(criteria::InteropRootsCriterion),
        Box::new(criteria::CircuitsCriterion),
        Box::new(criteria::TxEncodingSizeCriterion),
        Box::new(criteria::GasForBatchTipCriterion),
        Box::new(criteria::L1L2TxsCriterion),
        Box::new(criteria::L2L1LogsCriterion),
    ]
}

/// All sealers
fn default_sealers() -> Vec<Box<dyn SealCriterion>> {
    let mut sealers = validium_sealers();
    sealers.push(Box::new(criteria::PubDataBytesCriterion));
    sealers
}

impl SequencerSealer {
    pub fn new(config: SealCriteriaConfig) -> Self {
        let sealers = default_sealers();
        Self { config, sealers }
    }

    #[cfg(test)]
    pub(crate) fn with_sealers(
        config: SealCriteriaConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        Self { config, sealers }
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
        _interop_roots_count: usize,
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
        _interop_roots_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Vec<(&'static str, f64)> {
        Vec::new()
    }
}

/// Sealer for usage in EN. Panics if passed transaction should be excluded.
#[derive(Debug)]
pub struct PanicSealer {
    sealers: Vec<Box<dyn SealCriterion>>,
}

impl PanicSealer {
    pub fn new(l1_batch_commit_data_generator_mode: L1BatchCommitmentMode) -> Self {
        if l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium {
            Self {
                sealers: validium_sealers(),
            }
        } else {
            Self {
                sealers: default_sealers(),
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_sealers(sealers: Vec<Box<dyn SealCriterion>>) -> Self {
        Self { sealers }
    }
}

impl PanicSealer {
    fn seal_criteria_config_for_protocol_version(
        &self,
        protocol_version: ProtocolVersionId,
    ) -> SealCriteriaConfig {
        SealCriteriaConfig {
            transaction_slots: get_bootloader_max_txs_in_batch(protocol_version.into()), // do not limit on transaction slots
            max_pubdata_per_batch: (get_max_vm_pubdata_per_batch(protocol_version.into()) as u64)
                .into(),
            reject_tx_at_geometry_percentage: 1.0,
            reject_tx_at_eth_params_percentage: 1.0,
            reject_tx_at_gas_percentage: 1.0,
            close_block_at_geometry_percentage: 1.0,
            close_block_at_eth_params_percentage: 1.0,
            close_block_at_gas_percentage: 1.0,
            max_circuits_per_batch: get_max_batch_base_layer_circuits(protocol_version.into()),
        }
    }
}

impl ConditionalSealer for PanicSealer {
    fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        tx_count: usize,
        l1_tx_count: usize,
        interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution {
        tracing::trace!(
            "Checking seal resolution for L1 batch #{l1_batch_number} with {tx_count} transactions \
             and metrics {:?}",
            block_data.execution_metrics
        );

        // This seal config represents max criteria that we can allow. Sequencer may have stricter seal config
        let en_seal_config = self.seal_criteria_config_for_protocol_version(protocol_version);

        for sealer in &self.sealers {
            let seal_resolution = sealer.should_seal(
                &en_seal_config,
                tx_count,
                l1_tx_count,
                interop_roots_count,
                block_data,
                tx_data,
                protocol_version,
            );
            match &seal_resolution {
                SealResolution::IncludeAndSeal | SealResolution::NoSeal => {
                    // no seal, don't need to do anything
                }
                SealResolution::ExcludeAndSeal => {
                    panic!("Transaction should have been excluded, but was sequenced");
                }
                SealResolution::Unexecutable(reason) => {
                    panic!("Unexecutable transaction was sequenced: {reason:?}");
                }
            }
        }
        SealResolution::NoSeal // EN sealer either panics when we should seal or reports "NoSeal"
    }

    fn capacity_filled(
        &self,
        tx_count: usize,
        l1_tx_count: usize,
        interop_roots_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> Vec<(&'static str, f64)> {
        self.sealers
            .iter()
            .filter_map(|s| {
                let filled = s.capacity_filled(
                    &self.seal_criteria_config_for_protocol_version(protocol_version),
                    tx_count,
                    l1_tx_count,
                    interop_roots_count,
                    block_data,
                    protocol_version,
                );
                filled.map(|f| (s.prom_criterion_name(), f))
            })
            .collect()
    }
}
