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

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_types::{
    block::BlockGasCount,
    fee::TransactionExecutionMetrics,
    tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics},
    Transaction,
};
use zksync_utils::time::millis_since;

mod conditional_sealer;
pub(super) mod criteria;

pub(crate) use self::conditional_sealer::ConditionalSealer;
use super::{extractors, updates::UpdatesManager};
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

    /// Name of this resolution usable as a metric label.
    pub fn name(&self) -> &'static str {
        match self {
            Self::NoSeal => "no_seal",
            Self::IncludeAndSeal => "include_and_seal",
            Self::ExcludeAndSeal => "exclude_and_seal",
            Self::Unexecutable(_) => "unexecutable",
        }
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
    ) -> Self {
        let execution_metrics = ExecutionMetrics::from_tx_metrics(tx_metrics);
        let writes_metrics = DeduplicatedWritesMetrics::from_tx_metrics(tx_metrics);
        let gas_count = gas_count_from_tx_and_metrics(&transaction, &execution_metrics)
            + gas_count_from_writes(&writes_metrics);
        Self {
            execution_metrics,
            gas_count,
            cumulative_size: extractors::encoded_transaction_size(transaction),
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

impl fmt::Debug for SealManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealManager")
            .finish_non_exhaustive()
    }
}

impl SealManager {
    /// Creates a default pre-configured seal manager for the main node.
    pub(super) fn new(config: StateKeeperConfig) -> Self {
        let timeout_batch_sealer = Self::timeout_batch_sealer(config.block_commit_deadline_ms);
        let timeout_miniblock_sealer =
            Self::timeout_miniblock_sealer(config.miniblock_commit_deadline_ms);
        // Currently, it's assumed that timeout is the only criterion for miniblock sealing.
        // If this doesn't hold and some miniblocks are sealed in less than 1 second,
        // then state keeper will be blocked waiting for the miniblock timestamp to be changed.
        let miniblock_sealers = vec![timeout_miniblock_sealer];

        let conditional_sealer = ConditionalSealer::new(config);

        Self::custom(
            Some(conditional_sealer),
            vec![timeout_batch_sealer],
            miniblock_sealers,
        )
    }

    /// Allows to create a seal manager object from externally-defined sealers.
    pub fn custom(
        conditional_sealer: Option<ConditionalSealer>,
        unconditional_sealers: Vec<Box<SealerFn>>,
        miniblock_sealers: Vec<Box<SealerFn>>,
    ) -> Self {
        Self {
            conditional_sealer,
            unconditional_sealers,
            miniblock_sealers,
        }
    }

    /// Creates a sealer function that would seal the batch because of the timeout.
    fn timeout_batch_sealer(block_commit_deadline_ms: u64) -> Box<SealerFn> {
        const RULE_NAME: &str = "no_txs_timeout";

        Box::new(move |manager| {
            // Verify timestamp
            let should_seal_timeout =
                millis_since(manager.batch_timestamp()) > block_commit_deadline_ms;

            if should_seal_timeout {
                metrics::increment_counter!("server.tx_aggregation.reason", "criterion" => RULE_NAME);
                vlog::debug!(
                    "Decided to seal L1 batch using rule `{RULE_NAME}`; batch timestamp: {}, \
                     commit deadline: {block_commit_deadline_ms}ms",
                    extractors::display_timestamp(manager.batch_timestamp())
                );
            }
            should_seal_timeout
        })
    }

    /// Creates a sealer function that would seal the miniblock because of the timeout.
    /// Will only trigger for the non-empty miniblocks.
    fn timeout_miniblock_sealer(miniblock_commit_deadline_ms: u64) -> Box<SealerFn> {
        if miniblock_commit_deadline_ms < 1000 {
            panic!("`miniblock_commit_deadline_ms` should be at least 1000, because miniblocks must have different timestamps");
        }

        Box::new(move |manager| {
            !manager.miniblock.executed_transactions.is_empty()
                && millis_since(manager.miniblock.timestamp) > miniblock_commit_deadline_ms
        })
    }

    pub(super) fn should_seal_l1_batch(
        &self,
        l1_batch_number: u32,
        block_open_timestamp_ms: u128,
        tx_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
    ) -> SealResolution {
        if let Some(sealer) = &self.conditional_sealer {
            sealer.should_seal_l1_batch(
                l1_batch_number,
                block_open_timestamp_ms,
                tx_count,
                block_data,
                tx_data,
            )
        } else {
            SealResolution::NoSeal
        }
    }

    pub(super) fn should_seal_l1_batch_unconditionally(
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

    pub(super) fn should_seal_miniblock(&self, updates_manager: &UpdatesManager) -> bool {
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
        );
    }

    /// This test mostly exists to make sure that we can't seal empty miniblocks on the main node.
    #[test]
    fn timeout_miniblock_sealer() {
        let timeout_miniblock_sealer = SealManager::timeout_miniblock_sealer(10_000);

        let mut manager = create_updates_manager();
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
