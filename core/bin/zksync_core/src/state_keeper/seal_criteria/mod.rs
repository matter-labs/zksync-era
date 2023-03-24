//! Seal criteria is a module system for checking whether a currently open block must be sealed.
//!
//! Criteria for sealing may vary, for example:
//!
//! - No transactions slots left.
//! - We've reached timeout for sealing block.
//! - We've reached timeout for sealing *aggregated* block.
//! - We won't fit into the acceptable gas limit with any more transactions.
//!
//! Maintaining all the criteria in one place has proven itself to be very error-prone,
//! thus now every criterion is independent of the others.

use std::fmt::Debug;
pub(self) use zksync_config::configs::chain::StateKeeperConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::block::BlockGasCount;
use zksync_types::tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics};
use zksync_utils::time::{millis_since, millis_since_epoch};

use super::updates::UpdatesManager;

pub(crate) mod function;
pub(crate) mod gas;
mod geometry_seal_criteria;
mod pubdata_bytes;
pub(crate) mod slots;
mod timeout;
mod tx_encoding_size;

/// Reported decision regarding block sealing.
#[derive(Debug, Clone, PartialEq)]
pub enum SealResolution {
    /// Block should not be sealed right now.
    NoSeal,
    /// Latest transaction should be included into the block and sealed after that.
    IncludeAndSeal,
    /// Latest transaction should be excluded from the block and become the first
    /// tx in the next block.
    /// While it may be kinda counter-intuitive that we first execute transaction and just then
    /// decided whether we should include it into the block or not, it is required by the architecture of
    /// zkSync Era. We may not know, for example, how much gas block will consume, because 1) smart contract
    /// execution is hard to predict and 2) we may have writes to the same storage slots, which will save us
    /// gas.
    ExcludeAndSeal,
    /// Unexecutable means that the last transaction of the block cannot be executed even
    /// if the block will consist of it solely. Such a transaction must be rejected.
    ///
    /// Contains a reason for why transaction was considered unexecutable.
    Unexecutable(String),
}

impl SealResolution {
    /// Compares two seal resolutions and chooses the one that is stricter.
    /// `Unexecutable` is stricter than `ExcludeAndSeal`.
    /// `ExcludeAndSeal` is stricter than `IncludeAndSeal`.
    /// `IncludeAndSeal` is stricter than `NoSeal`.
    pub fn stricter(self, other: SealResolution) -> SealResolution {
        match (self, other) {
            (SealResolution::Unexecutable(reason), _)
            | (_, SealResolution::Unexecutable(reason)) => SealResolution::Unexecutable(reason),
            (SealResolution::ExcludeAndSeal, _) | (_, SealResolution::ExcludeAndSeal) => {
                SealResolution::ExcludeAndSeal
            }
            (SealResolution::IncludeAndSeal, _) | (_, SealResolution::IncludeAndSeal) => {
                SealResolution::IncludeAndSeal
            }
            _ => SealResolution::NoSeal,
        }
    }

    /// Returns `true` if L1 batch should be sealed according to this resolution.
    pub fn should_seal(self) -> bool {
        matches!(
            self,
            SealResolution::IncludeAndSeal | SealResolution::ExcludeAndSeal
        )
    }
}

pub trait SealCriterion: Debug + Send + 'static {
    #[allow(clippy::too_many_arguments)]
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
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
    ) -> SealResolution;
    // We need self here only for rust restrictions for creating an object from trait
    // https://doc.rust-lang.org/reference/items/traits.html#object-safety
    fn prom_criterion_name(&self) -> &'static str;
}

/// Sealer function that returns a boolean.
pub type SealerFn = dyn Fn(&UpdatesManager) -> bool + Send;

pub struct SealManager {
    config: StateKeeperConfig,
    /// Primary sealers set that is used to check if batch should be sealed after executing a transaction.
    sealers: Vec<Box<dyn SealCriterion>>,
    /// Unconditional batch sealer, i.e. one that can be used if we should seal the batch *without* executing a tx.
    unconditional_sealer: Box<SealerFn>,
    /// Miniblock sealer function used to determine if we should seal the miniblock.
    miniblock_sealer: Box<SealerFn>,
}

impl Debug for SealManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealManager")
            .field("config", &self.config)
            .field("sealers", &self.sealers)
            .finish()
    }
}

impl SealManager {
    /// Creates a default pre-configured seal manager.
    pub(crate) fn new(config: StateKeeperConfig) -> Self {
        let sealers: Vec<Box<dyn SealCriterion>> = Self::get_default_sealers();
        let unconditional_sealer = Self::timeout_and_code_hash_batch_sealer(
            config.block_commit_deadline_ms,
            BaseSystemContractsHashes {
                bootloader: config.bootloader_hash,
                default_aa: config.default_aa_hash,
            },
        );
        let miniblock_sealer = Self::timeout_miniblock_sealer(config.miniblock_commit_deadline_ms);

        Self::custom(config, sealers, unconditional_sealer, miniblock_sealer)
    }

    /// Allows to create a seal manager object from externally-defined sealers.
    /// Mostly useful for test configuration.
    pub fn custom(
        config: StateKeeperConfig,
        sealers: Vec<Box<dyn SealCriterion>>,
        unconditional_sealer: Box<SealerFn>,
        miniblock_sealer: Box<SealerFn>,
    ) -> Self {
        Self {
            config,
            sealers,
            unconditional_sealer,
            miniblock_sealer,
        }
    }

    /// Creates a sealer function that would seal the batch because of the timeout.
    pub(crate) fn timeout_and_code_hash_batch_sealer(
        block_commit_deadline_ms: u64,
        base_system_contracts_hashes: BaseSystemContractsHashes,
    ) -> Box<SealerFn> {
        Box::new(move |manager| {
            // Verify timestamp
            let should_seal_timeout =
                millis_since(manager.batch_timestamp()) > block_commit_deadline_ms;
            // Verify code hashes
            let should_seal_code_hashes =
                base_system_contracts_hashes != manager.base_system_contract_hashes();

            if should_seal_timeout {
                metrics::increment_counter!(
                    "server.tx_aggregation.reason",
                    "criterion" => "no_txs_timeout"
                );
                vlog::info!(
                    "l1_batch_timeout_triggered without new txs: {:?} {:?} {:?}",
                    manager.batch_timestamp(),
                    block_commit_deadline_ms,
                    millis_since_epoch()
                );
            }

            if should_seal_code_hashes {
                metrics::increment_counter!(
                    "server.tx_aggregation.reason",
                    "criterion" => "different_code_hashes"
                );
                vlog::info!(
                    "l1_batch_different_code_hashes_triggered without new txs \n
                    l1 batch code hashes: {:?} \n
                    expected code hashes {:?} ",
                    base_system_contracts_hashes,
                    manager.base_system_contract_hashes(),
                );
            }

            should_seal_timeout || should_seal_code_hashes
        })
    }

    /// Creates a sealer function that would seal the miniblock because of the timeout.
    /// Will only trigger for the non-empty miniblocks.
    fn timeout_miniblock_sealer(miniblock_commit_deadline_ms: u64) -> Box<SealerFn> {
        Box::new(move |manager| {
            !manager.miniblock.executed_transactions.is_empty()
                && millis_since(manager.miniblock.timestamp) > miniblock_commit_deadline_ms
        })
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

    pub(crate) fn should_seal_l1_batch_unconditionally(
        &self,
        updates_manager: &UpdatesManager,
    ) -> bool {
        updates_manager.pending_executed_transactions_len() != 0
            && (self.unconditional_sealer)(updates_manager)
    }

    pub(crate) fn should_seal_miniblock(&self, updates_manager: &UpdatesManager) -> bool {
        // Unlike with the L1 batch, we don't check the number of transactions in the miniblock,
        // because we might want to seal the miniblock even if it's empty (e.g. on an external node,
        // where we have to replicate the state of the main node, including the last (empty) miniblock of the batch).
        // The check for the number of transactions is expected to be done, if relevant, in the `miniblock_sealer`
        // directly.
        (self.miniblock_sealer)(updates_manager)
    }

    pub(crate) fn get_default_sealers() -> Vec<Box<dyn SealCriterion>> {
        let sealers: Vec<Box<dyn SealCriterion>> = vec![
            Box::new(slots::SlotsCriterion),
            Box::new(gas::GasCriterion),
            Box::new(pubdata_bytes::PubDataBytesCriterion),
            Box::new(geometry_seal_criteria::BytecodeHashesCriterion),
            Box::new(geometry_seal_criteria::InitialWritesCriterion),
            Box::new(geometry_seal_criteria::RepeatedWritesCriterion),
            Box::new(geometry_seal_criteria::MaxCyclesCriterion),
            Box::new(tx_encoding_size::TxEncodingSizeCriterion),
        ];
        sealers
    }
}

#[cfg(test)]
mod tests {
    use vm::{
        vm::{VmPartialExecutionResult, VmTxExecutionResult},
        vm_with_bootloader::{BlockContext, BlockContextMode, DerivedBlockContext},
    };
    use zksync_types::{
        l2::L2Tx,
        tx::tx_execution_info::{TxExecutionStatus, VmExecutionLogs},
        Address, Nonce, H256, U256,
    };
    use zksync_utils::time::seconds_since_epoch;

    use super::*;

    fn create_manager() -> UpdatesManager {
        let block_context = BlockContextMode::NewBlock(
            DerivedBlockContext {
                context: BlockContext {
                    block_number: 0,
                    block_timestamp: 0,
                    l1_gas_price: 0,
                    fair_l2_gas_price: 0,
                    operator_address: Default::default(),
                },
                base_fee: 0,
            },
            0.into(),
        );
        UpdatesManager::new(&block_context, Default::default())
    }

    fn apply_tx_to_manager(manager: &mut UpdatesManager) {
        let mut tx = L2Tx::new(
            Default::default(),
            Default::default(),
            Nonce(0),
            Default::default(),
            Address::default(),
            U256::zero(),
            None,
            Default::default(),
        );
        tx.set_input(H256::random().0.to_vec(), H256::random());
        manager.extend_from_executed_transaction(
            &tx.into(),
            VmTxExecutionResult {
                status: TxExecutionStatus::Success,
                result: VmPartialExecutionResult {
                    logs: VmExecutionLogs::default(),
                    revert_reason: None,
                    contracts_used: 0,
                    cycles_used: 0,
                },
                gas_refunded: 0,
                operator_suggested_refund: 0,
            },
            Default::default(),
            Default::default(),
            Default::default(),
        );
    }

    /// This test mostly exists to make sure that we can't seal empty miniblocks on the main node.
    #[test]
    fn timeout_miniblock_sealer() {
        let timeout_miniblock_sealer = SealManager::timeout_miniblock_sealer(1000);

        let mut manager = create_manager();
        // Empty miniblock should not trigger.
        manager.miniblock.timestamp = seconds_since_epoch() - 10;
        assert!(
            !timeout_miniblock_sealer(&manager),
            "Empty miniblock shouldn't be sealed"
        );

        // Non-empty miniblock should trigger.
        apply_tx_to_manager(&mut manager);
        assert!(
            timeout_miniblock_sealer(&manager),
            "Non-empty miniblock with old timestamp should be sealed"
        );

        // Check the timestamp logic.
        manager.miniblock.timestamp = seconds_since_epoch();
        assert!(
            !timeout_miniblock_sealer(&manager),
            "Non-empty miniblock with too recent timestamp shouldn't be sealed"
        );
    }
}
