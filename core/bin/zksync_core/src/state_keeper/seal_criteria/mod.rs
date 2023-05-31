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

use self::conditional_sealer::ConditionalSealer;
use super::updates::UpdatesManager;

pub(crate) mod conditional_sealer;
pub(crate) mod criteria;

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
    /// Conditional sealer, i.e. one that can decide whether the batch should be sealed after executing a tx.
    /// Currently, it's expected to be `Some` on the main node and `None` on the external nodes, since external nodes
    /// do not decide whether to seal the batch or not.
    conditional_sealer: Option<ConditionalSealer>,
    /// Unconditional batch sealer, i.e. one that can be used if we should seal the batch *without* executing a tx.
    /// If any of the unconditional sealers returns `true`, the batch will be sealed.
    ///
    /// Note: only non-empty batch can be sealed.
    unconditional_sealers: Vec<Box<SealerFn>>,
    /// Miniblock sealer function used to determine if we should seal the miniblock.
    /// If any of the miniblock sealers returns `true`, the miniblock will be sealed.
    miniblock_sealers: Vec<Box<SealerFn>>,
}

impl Debug for SealManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealManager").finish()
    }
}

impl SealManager {
    /// Creates a default pre-configured seal manager for the main node.
    pub(crate) fn new(config: StateKeeperConfig) -> Self {
        let timeout_batch_sealer = Self::timeout_batch_sealer(config.block_commit_deadline_ms);
        let code_hash_batch_sealer = Self::code_hash_batch_sealer(BaseSystemContractsHashes {
            bootloader: config.bootloader_hash,
            default_aa: config.default_aa_hash,
        });
        let timeout_miniblock_sealer =
            Self::timeout_miniblock_sealer(config.miniblock_commit_deadline_ms);
        let conditional_sealer = ConditionalSealer::new(config);

        Self::custom(
            Some(conditional_sealer),
            vec![timeout_batch_sealer, code_hash_batch_sealer],
            vec![timeout_miniblock_sealer],
        )
    }

    /// Allows to create a seal manager object from externally-defined sealers.
    pub fn custom(
        conditional_sealer: Option<ConditionalSealer>,
        unconditional_sealer: Vec<Box<SealerFn>>,
        miniblock_sealer: Vec<Box<SealerFn>>,
    ) -> Self {
        Self {
            conditional_sealer,
            unconditional_sealers: unconditional_sealer,
            miniblock_sealers: miniblock_sealer,
        }
    }

    /// Creates a sealer function that would seal the batch because of the timeout.
    pub(crate) fn timeout_batch_sealer(block_commit_deadline_ms: u64) -> Box<SealerFn> {
        Box::new(move |manager| {
            // Verify timestamp
            let should_seal_timeout =
                millis_since(manager.batch_timestamp()) > block_commit_deadline_ms;

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

            should_seal_timeout
        })
    }

    /// Creates a sealer function that would seal the batch if the provided base system contract hashes are different
    /// from ones in the updates manager.
    pub(crate) fn code_hash_batch_sealer(
        base_system_contracts_hashes: BaseSystemContractsHashes,
    ) -> Box<SealerFn> {
        Box::new(move |manager| {
            // Verify code hashes
            let should_seal_code_hashes =
                base_system_contracts_hashes != manager.base_system_contract_hashes();

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

            should_seal_code_hashes
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
        if let Some(sealer) = self.conditional_sealer.as_ref() {
            sealer.should_seal_l1_batch(
                l1_batch_number,
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
            )
        } else {
            SealResolution::NoSeal
        }
    }

    pub(crate) fn should_seal_l1_batch_unconditionally(
        &self,
        updates_manager: &UpdatesManager,
    ) -> bool {
        // Regardless of which sealers are provided, we never want to seal an empty batch.
        updates_manager.pending_executed_transactions_len() != 0
            && self
                .unconditional_sealers
                .iter()
                .any(|sealer| (sealer)(updates_manager))
    }

    pub(crate) fn should_seal_miniblock(&self, updates_manager: &UpdatesManager) -> bool {
        // Unlike with the L1 batch, we don't check the number of transactions in the miniblock,
        // because we might want to seal the miniblock even if it's empty (e.g. on an external node,
        // where we have to replicate the state of the main node, including the last (empty) miniblock of the batch).
        // The check for the number of transactions is expected to be done, if relevant, in the `miniblock_sealer`
        // directly.
        self.miniblock_sealers
            .iter()
            .any(|sealer| (sealer)(updates_manager))
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
                    computational_gas_used: 0,
                },
                call_traces: vec![],
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
        let timeout_miniblock_sealer = SealManager::timeout_miniblock_sealer(10_000);

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

        // Check the timestamp logic. This relies on the fact that the test shouldn't run
        // for more than 10 seconds (while the test itself is trivial, it may be preempted
        // by other tests).
        manager.miniblock.timestamp = seconds_since_epoch();
        assert!(
            !timeout_miniblock_sealer(&manager),
            "Non-empty miniblock with too recent timestamp shouldn't be sealed"
        );
    }
}
