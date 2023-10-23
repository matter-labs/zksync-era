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

use std::fmt;

use multivm::vm_latest::TransactionVmExt;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::{
    block::BlockGasCount,
    fee::TransactionExecutionMetrics,
    tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics},
    ProtocolVersionId, Transaction,
};
use zksync_utils::time::millis_since;

mod conditional_sealer;
pub(super) mod criteria;

pub(crate) use self::conditional_sealer::ConditionalSealer;
use super::{extractors, metrics::AGGREGATION_METRICS, updates::UpdatesManager};
use crate::gas_tracker::{gas_count_from_tx_and_metrics, gas_count_from_writes};

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
    pub fn stricter(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unexecutable(reason), _) | (_, Self::Unexecutable(reason)) => {
                Self::Unexecutable(reason)
            }
            (Self::ExcludeAndSeal, _) | (_, Self::ExcludeAndSeal) => Self::ExcludeAndSeal,
            (Self::IncludeAndSeal, _) | (_, Self::IncludeAndSeal) => Self::IncludeAndSeal,
            _ => Self::NoSeal,
        }
    }

    /// Returns `true` if L1 batch should be sealed according to this resolution.
    pub fn should_seal(&self) -> bool {
        matches!(self, Self::IncludeAndSeal | Self::ExcludeAndSeal)
    }
}

/// Information about transaction or block applicable either to a single transaction, or
/// to the entire miniblock / L1 batch.
#[derive(Debug, Default)]
pub struct SealData {
    pub(super) execution_metrics: ExecutionMetrics,
    pub(super) gas_count: BlockGasCount,
    pub(super) cumulative_size: usize,
    pub(super) writes_metrics: DeduplicatedWritesMetrics,
}

impl SealData {
    /// Creates sealing data based on the execution of a `transaction`. Assumes that all writes
    /// performed by the transaction are initial.
    pub(crate) fn for_transaction(
        transaction: Transaction,
        tx_metrics: &TransactionExecutionMetrics,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        let execution_metrics = ExecutionMetrics::from_tx_metrics(tx_metrics);
        let writes_metrics = DeduplicatedWritesMetrics::from_tx_metrics(tx_metrics);
        let gas_count = gas_count_from_tx_and_metrics(&transaction, &execution_metrics)
            + gas_count_from_writes(&writes_metrics, protocol_version);
        Self {
            execution_metrics,
            gas_count,
            cumulative_size: transaction.bootloader_encoding_size(),
            writes_metrics,
        }
    }
}

pub(super) trait SealCriterion: fmt::Debug + Send + 'static {
    fn should_seal(
        &self,
        config: &StateKeeperConfig,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution;

    // We need self here only for rust restrictions for creating an object from trait
    // https://doc.rust-lang.org/reference/items/traits.html#object-safety
    fn prom_criterion_name(&self) -> &'static str;
}

/// I/O-dependent seal criteria.
pub trait IoSealCriteria {
    /// Checks whether an L1 batch should be sealed unconditionally (i.e., regardless of metrics
    /// related to transaction execution) given the provided `manager` state.
    fn should_seal_l1_batch_unconditionally(&mut self, manager: &UpdatesManager) -> bool;
    /// Checks whether a miniblock should be sealed given the provided `manager` state.
    fn should_seal_miniblock(&mut self, manager: &UpdatesManager) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TimeoutSealer {
    block_commit_deadline_ms: u64,
    miniblock_commit_deadline_ms: u64,
}

impl TimeoutSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            block_commit_deadline_ms: config.block_commit_deadline_ms,
            miniblock_commit_deadline_ms: config.miniblock_commit_deadline_ms,
        }
    }
}

impl IoSealCriteria for TimeoutSealer {
    fn should_seal_l1_batch_unconditionally(&mut self, manager: &UpdatesManager) -> bool {
        const RULE_NAME: &str = "no_txs_timeout";

        if manager.pending_executed_transactions_len() == 0 {
            // Regardless of which sealers are provided, we never want to seal an empty batch.
            return false;
        }

        let block_commit_deadline_ms = self.block_commit_deadline_ms;
        // Verify timestamp
        let should_seal_timeout =
            millis_since(manager.batch_timestamp()) > block_commit_deadline_ms;

        if should_seal_timeout {
            AGGREGATION_METRICS.inc_criterion(RULE_NAME);
            tracing::debug!(
                "Decided to seal L1 batch using rule `{RULE_NAME}`; batch timestamp: {}, \
                 commit deadline: {block_commit_deadline_ms}ms",
                extractors::display_timestamp(manager.batch_timestamp())
            );
        }
        should_seal_timeout
    }

    fn should_seal_miniblock(&mut self, manager: &UpdatesManager) -> bool {
        !manager.miniblock.executed_transactions.is_empty()
            && millis_since(manager.miniblock.timestamp) > self.miniblock_commit_deadline_ms
    }
}

#[cfg(test)]
mod tests {
    use zksync_utils::time::seconds_since_epoch;

    use super::*;
    use crate::state_keeper::tests::{
        create_execution_result, create_transaction, create_updates_manager,
    };

    fn apply_tx_to_manager(manager: &mut UpdatesManager) {
        let tx = create_transaction(10, 100);
        manager.extend_from_executed_transaction(
            tx,
            create_execution_result(0, []),
            vec![],
            BlockGasCount::default(),
            ExecutionMetrics::default(),
            vec![],
        );
    }

    /// This test mostly exists to make sure that we can't seal empty miniblocks on the main node.
    #[test]
    fn timeout_miniblock_sealer() {
        let mut timeout_miniblock_sealer = TimeoutSealer {
            block_commit_deadline_ms: 10_000,
            miniblock_commit_deadline_ms: 10_000,
        };

        let mut manager = create_updates_manager();
        // Empty miniblock should not trigger.
        manager.miniblock.timestamp = seconds_since_epoch() - 10;
        assert!(
            !timeout_miniblock_sealer.should_seal_miniblock(&manager),
            "Empty miniblock shouldn't be sealed"
        );

        // Non-empty miniblock should trigger.
        apply_tx_to_manager(&mut manager);
        assert!(
            timeout_miniblock_sealer.should_seal_miniblock(&manager),
            "Non-empty miniblock with old timestamp should be sealed"
        );

        // Check the timestamp logic. This relies on the fact that the test shouldn't run
        // for more than 10 seconds (while the test itself is trivial, it may be preempted
        // by other tests).
        manager.miniblock.timestamp = seconds_since_epoch();
        assert!(
            !timeout_miniblock_sealer.should_seal_miniblock(&manager),
            "Non-empty miniblock with too recent timestamp shouldn't be sealed"
        );
    }
}
