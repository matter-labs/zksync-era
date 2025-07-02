//! This module represents the conditional sealer, which can decide whether the batch
//! should be sealed after executing a particular transaction.
//!
//! The conditional sealer abstraction allows to implement different sealing strategies, e.g. the actual
//! sealing strategy for the main node or noop sealer for the external node.

use std::fmt;

use async_trait::async_trait;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_multivm::{
    interface::TransactionExecutionMetrics,
    utils::{get_bootloader_max_txs_in_batch, get_max_batch_base_layer_circuits},
};
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
        max_pubdata_per_batch: Option<usize>,
    ) -> SealResolution;

    /// Returns fractions of the criteria's capacity filled in the batch.
    fn capacity_filled(
        &self,
        tx_count: usize,
        l1_tx_count: usize,
        block_data: &SealData,
        protocol_version: ProtocolVersionId,
        max_pubdata_per_batch: Option<usize>,
    ) -> Vec<(&'static str, f64)>;

    fn handle_is_active_leader_change(&mut self, _is_leader: bool) {}

    fn set_config(&mut self, _protocol_version: ProtocolVersionId) {}
}

#[derive(Debug, Clone, Copy)]
pub struct L1BatchSealConfig {
    pub max_circuits_per_batch: usize,
    pub transaction_slots: usize,
    pub close_block_at_geometry_percentage: f64,
    pub reject_tx_at_geometry_percentage: f64,
    pub close_block_at_eth_params_percentage: f64,
    pub reject_tx_at_eth_params_percentage: f64,
}

impl From<StateKeeperConfig> for L1BatchSealConfig {
    fn from(config: StateKeeperConfig) -> Self {
        Self {
            max_circuits_per_batch: config.max_circuits_per_batch,
            transaction_slots: config.transaction_slots,
            close_block_at_geometry_percentage: config.close_block_at_geometry_percentage,
            reject_tx_at_geometry_percentage: config.reject_tx_at_geometry_percentage,
            close_block_at_eth_params_percentage: config.close_block_at_eth_params_percentage,
            reject_tx_at_eth_params_percentage: config.reject_tx_at_eth_params_percentage,
        }
    }
}

impl L1BatchSealConfig {
    pub fn max(protocol_version: ProtocolVersionId) -> Self {
        Self {
            max_circuits_per_batch: get_max_batch_base_layer_circuits(protocol_version.into()),
            transaction_slots: get_bootloader_max_txs_in_batch(protocol_version.into()),
            close_block_at_geometry_percentage: 1.0,
            reject_tx_at_geometry_percentage: 1.0,
            close_block_at_eth_params_percentage: 1.0,
            reject_tx_at_eth_params_percentage: 1.0,
        }
    }

    pub fn for_tests() -> Self {
        StateKeeperConfig::for_tests().into()
    }
}

/// Implementation of [`ConditionalSealer`] used by the main node.
/// Internally uses a set of [`SealCriterion`]s to determine whether the batch should be sealed.
///
/// The checks are deterministic, i.e., should depend solely on execution metrics and [`L1BatchSealConfig`].
/// Non-deterministic seal criteria are expressed using [`IoSealCriteria`](super::IoSealCriteria).
#[derive(Debug)]
pub struct SequencerSealer {
    local_config: L1BatchSealConfig,
    local_max_pubdata_per_batch: usize,
    current_config: Option<L1BatchSealConfig>,
    is_active_leader: bool,
    sealers: Vec<Box<dyn SealCriterion>>,
}

#[cfg(test)]
impl SequencerSealer {
    pub(crate) fn for_tests() -> Self {
        Self {
            local_config: L1BatchSealConfig::for_tests(),
            local_max_pubdata_per_batch: 100_000,
            current_config: Some(L1BatchSealConfig::for_tests()),
            is_active_leader: true,
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
                &self.current_config.unwrap(),
                TX_COUNT,
                TX_COUNT,
                &data,
                &data,
                ProtocolVersionId::latest(),
                self.local_max_pubdata_per_batch,
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
        max_pubdata_per_batch: Option<usize>,
    ) -> SealResolution {
        tracing::trace!(
            "Determining seal resolution for L1 batch #{l1_batch_number} with {tx_count} transactions \
             and metrics {:?}",
            block_data.execution_metrics
        );

        let mut final_seal_resolution = SealResolution::NoSeal;
        for sealer in &self.sealers {
            let seal_resolution = sealer.should_seal(
                &self.current_config.unwrap(),
                tx_count,
                l1_tx_count,
                block_data,
                tx_data,
                protocol_version,
                max_pubdata_per_batch.unwrap_or(self.local_max_pubdata_per_batch),
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
        max_pubdata_per_batch: Option<usize>,
    ) -> Vec<(&'static str, f64)> {
        self.sealers
            .iter()
            .filter_map(|s| {
                let filled = s.capacity_filled(
                    &self.current_config.unwrap(),
                    tx_count,
                    l1_tx_count,
                    block_data,
                    protocol_version,
                    max_pubdata_per_batch.unwrap_or(self.local_max_pubdata_per_batch),
                );
                filled.map(|f| (s.prom_criterion_name(), f))
            })
            .collect()
    }

    fn handle_is_active_leader_change(&mut self, is_leader: bool) {
        self.is_active_leader = is_leader;
        // Config will be set in `set_config` later.
        self.current_config = None;
    }

    fn set_config(&mut self, protocol_version: ProtocolVersionId) {
        if self.is_active_leader {
            self.current_config = Some(self.local_config);
        } else {
            self.current_config = Some(L1BatchSealConfig::max(protocol_version));
        }
    }
}

impl SequencerSealer {
    pub fn new(sk_config: StateKeeperConfig) -> Self {
        let local_max_pubdata_per_batch = sk_config.max_pubdata_per_batch.0 as usize;
        let config: L1BatchSealConfig = sk_config.into();
        let sealers = Self::default_sealers();
        Self {
            local_config: config,
            local_max_pubdata_per_batch,
            current_config: Some(config),
            is_active_leader: true,
            sealers,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_sealers(
        sk_config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
    ) -> Self {
        let local_max_pubdata_per_batch = sk_config.max_pubdata_per_batch.0 as usize;
        let config: L1BatchSealConfig = sk_config.into();
        Self {
            local_config: config,
            local_max_pubdata_per_batch,
            current_config: Some(config),
            is_active_leader: true,
            sealers,
        }
    }

    fn default_sealers() -> Vec<Box<dyn SealCriterion>> {
        vec![
            Box::new(criteria::SlotsCriterion),
            Box::new(criteria::PubDataBytesCriterion),
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
        _max_pubdata_per_batch: Option<usize>,
    ) -> SealResolution {
        SealResolution::NoSeal
    }

    fn capacity_filled(
        &self,
        _tx_count: usize,
        _l1_tx_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
        _max_pubdata_per_batch: Option<usize>,
    ) -> Vec<(&'static str, f64)> {
        Vec::new()
    }
}
