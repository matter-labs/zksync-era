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
use zksync_multivm::{
    interface::{DeduplicatedWritesMetrics, Halt, TransactionExecutionMetrics, VmExecutionMetrics},
    vm_latest::TransactionVmExt,
};
use zksync_types::{
    block::BlockGasCount, utils::display_timestamp, ProtocolVersionId, Transaction,
};
use zksync_utils::time::millis_since;

mod conditional_sealer;
pub(super) mod criteria;

pub use self::conditional_sealer::{ConditionalSealer, NoopSealer, SequencerSealer};
use super::{
    metrics::AGGREGATION_METRICS,
    updates::UpdatesManager,
    utils::{gas_count_from_tx_and_metrics, gas_count_from_writes},
};

fn halt_as_metric_label(halt: &Halt) -> &'static str {
    match halt {
        Halt::ValidationFailed(_) => "ValidationFailed",
        Halt::PaymasterValidationFailed(_) => "PaymasterValidationFailed",
        Halt::PrePaymasterPreparationFailed(_) => "PrePaymasterPreparationFailed",
        Halt::PayForTxFailed(_) => "PayForTxFailed",
        Halt::FailedToMarkFactoryDependencies(_) => "FailedToMarkFactoryDependencies",
        Halt::FailedToChargeFee(_) => "FailedToChargeFee",
        Halt::FromIsNotAnAccount => "FromIsNotAnAccount",
        Halt::InnerTxError => "InnerTxError",
        Halt::Unknown(_) => "Unknown",
        Halt::UnexpectedVMBehavior(_) => "UnexpectedVMBehavior",
        Halt::BootloaderOutOfGas => "BootloaderOutOfGas",
        Halt::ValidationOutOfGas => "ValidationOutOfGas",
        Halt::TooBigGasLimit => "TooBigGasLimit",
        Halt::NotEnoughGasProvided => "NotEnoughGasProvided",
        Halt::MissingInvocationLimitReached => "MissingInvocationLimitReached",
        Halt::FailedToSetL2Block(_) => "FailedToSetL2Block",
        Halt::FailedToAppendTransactionToL2Block(_) => "FailedToAppendTransactionToL2Block",
        Halt::VMPanic => "VMPanic",
        Halt::TracerCustom(_) => "TracerCustom",
        Halt::FailedToPublishCompressedBytecodes => "FailedToPublishCompressedBytecodes",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnexecutableReason {
    Halt(Halt),
    TxEncodingSize,
    LargeEncodingSize,
    PubdataLimit,
    ProofWillFail,
    TooMuchGas,
    OutOfGasForBatchTip,
    BootloaderOutOfGas,
    NotEnoughGasProvided,
}

impl UnexecutableReason {
    pub fn as_metric_label(&self) -> &'static str {
        match self {
            UnexecutableReason::Halt(halt) => halt_as_metric_label(halt),
            UnexecutableReason::TxEncodingSize => "TxEncodingSize",
            UnexecutableReason::LargeEncodingSize => "LargeEncodingSize",
            UnexecutableReason::PubdataLimit => "PubdataLimit",
            UnexecutableReason::ProofWillFail => "ProofWillFail",
            UnexecutableReason::TooMuchGas => "TooMuchGas",
            UnexecutableReason::OutOfGasForBatchTip => "OutOfGasForBatchTip",
            UnexecutableReason::BootloaderOutOfGas => "BootloaderOutOfGas",
            UnexecutableReason::NotEnoughGasProvided => "NotEnoughGasProvided",
        }
    }
}

impl From<UnexecutableReason> for SealResolution {
    fn from(reason: UnexecutableReason) -> Self {
        SealResolution::Unexecutable(reason)
    }
}

impl fmt::Display for UnexecutableReason {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UnexecutableReason::Halt(halt) => write!(f, "{}", halt),
            UnexecutableReason::TxEncodingSize => write!(f, "Transaction encoding size is too big"),
            UnexecutableReason::LargeEncodingSize => {
                write!(f, "Transaction encoding size is too big")
            }
            UnexecutableReason::PubdataLimit => write!(f, "Pubdata limit reached"),
            UnexecutableReason::ProofWillFail => write!(f, "Proof will fail"),
            UnexecutableReason::TooMuchGas => write!(f, "Too much gas"),
            UnexecutableReason::OutOfGasForBatchTip => write!(f, "Out of gas for batch tip"),
            UnexecutableReason::BootloaderOutOfGas => write!(f, "Bootloader out of gas"),
            UnexecutableReason::NotEnoughGasProvided => write!(f, "Not enough gas provided"),
        }
    }
}

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
    /// ZKsync Era. We may not know, for example, how much gas block will consume, because 1) smart contract
    /// execution is hard to predict and 2) we may have writes to the same storage slots, which will save us
    /// gas.
    ExcludeAndSeal,
    /// Unexecutable means that the last transaction of the block cannot be executed even
    /// if the block will consist of it solely. Such a transaction must be rejected.
    ///
    /// Contains a reason for why transaction was considered unexecutable.
    Unexecutable(UnexecutableReason),
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
/// to the entire L2 block / L1 batch.
#[derive(Debug, Default)]
pub struct SealData {
    pub(super) execution_metrics: VmExecutionMetrics,
    pub(super) gas_count: BlockGasCount,
    pub(super) cumulative_size: usize,
    pub(super) writes_metrics: DeduplicatedWritesMetrics,
    pub(super) gas_remaining: u32,
}

impl SealData {
    /// Creates sealing data based on the execution of a `transaction`. Assumes that all writes
    /// performed by the transaction are initial.
    pub fn for_transaction(
        transaction: &Transaction,
        tx_metrics: &TransactionExecutionMetrics,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        let execution_metrics = VmExecutionMetrics::from_tx_metrics(tx_metrics);
        let writes_metrics = DeduplicatedWritesMetrics::from_tx_metrics(tx_metrics);
        let gas_count = gas_count_from_tx_and_metrics(transaction, &execution_metrics)
            + gas_count_from_writes(&writes_metrics, protocol_version);
        Self {
            execution_metrics,
            gas_count,
            cumulative_size: transaction.bootloader_encoding_size(),
            writes_metrics,
            gas_remaining: tx_metrics.gas_remaining,
        }
    }
}

pub(super) trait SealCriterion: fmt::Debug + Send + Sync + 'static {
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
    /// Checks whether an L2 block should be sealed given the provided `manager` state.
    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TimeoutSealer {
    block_commit_deadline_ms: u64,
    l2_block_commit_deadline_ms: u64,
}

impl TimeoutSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            block_commit_deadline_ms: config.block_commit_deadline_ms,
            l2_block_commit_deadline_ms: config.l2_block_commit_deadline_ms,
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
            AGGREGATION_METRICS.l1_batch_reason_inc_criterion(RULE_NAME);
            tracing::debug!(
                "Decided to seal L1 batch using rule `{RULE_NAME}`; batch timestamp: {}, \
                 commit deadline: {block_commit_deadline_ms}ms",
                display_timestamp(manager.batch_timestamp())
            );
        }
        should_seal_timeout
    }

    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        !manager.l2_block.executed_transactions.is_empty()
            && millis_since(manager.l2_block.timestamp) > self.l2_block_commit_deadline_ms
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct L2BlockMaxPayloadSizeSealer {
    max_payload_size: usize,
}

impl L2BlockMaxPayloadSizeSealer {
    pub fn new(config: &StateKeeperConfig) -> Self {
        Self {
            max_payload_size: config.l2_block_max_payload_size,
        }
    }

    pub fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        manager.l2_block.payload_encoding_size >= self.max_payload_size
    }
}

#[cfg(test)]
mod tests {
    use zksync_utils::time::seconds_since_epoch;

    use super::*;
    use crate::tests::{create_execution_result, create_transaction, create_updates_manager};

    fn apply_tx_to_manager(tx: Transaction, manager: &mut UpdatesManager) {
        manager.extend_from_executed_transaction(
            tx,
            create_execution_result([]),
            vec![],
            vec![],
            BlockGasCount::default(),
            VmExecutionMetrics::default(),
            vec![],
        );
    }

    /// This test mostly exists to make sure that we can't seal empty L2 blocks on the main node.
    #[test]
    fn timeout_l2_block_sealer() {
        let mut timeout_l2_block_sealer = TimeoutSealer {
            block_commit_deadline_ms: 10_000,
            l2_block_commit_deadline_ms: 10_000,
        };

        let mut manager = create_updates_manager();
        // Empty L2 block should not trigger.
        manager.l2_block.timestamp = seconds_since_epoch() - 10;
        assert!(
            !timeout_l2_block_sealer.should_seal_l2_block(&manager),
            "Empty L2 block shouldn't be sealed"
        );

        // Non-empty L2 block should trigger.
        apply_tx_to_manager(create_transaction(10, 100), &mut manager);
        assert!(
            timeout_l2_block_sealer.should_seal_l2_block(&manager),
            "Non-empty L2 block with old timestamp should be sealed"
        );

        // Check the timestamp logic. This relies on the fact that the test shouldn't run
        // for more than 10 seconds (while the test itself is trivial, it may be preempted
        // by other tests).
        manager.l2_block.timestamp = seconds_since_epoch();
        assert!(
            !timeout_l2_block_sealer.should_seal_l2_block(&manager),
            "Non-empty L2 block with too recent timestamp shouldn't be sealed"
        );
    }

    #[test]
    fn max_size_l2_block_sealer() {
        let tx = create_transaction(10, 100);
        let tx_encoding_size =
            zksync_protobuf::repr::encode::<zksync_dal::consensus::proto::Transaction>(&tx).len();

        let mut max_payload_sealer = L2BlockMaxPayloadSizeSealer {
            max_payload_size: tx_encoding_size,
        };

        let mut manager = create_updates_manager();
        assert!(
            !max_payload_sealer.should_seal_l2_block(&manager),
            "Empty L2 block shouldn't be sealed"
        );

        apply_tx_to_manager(tx, &mut manager);
        assert!(
            max_payload_sealer.should_seal_l2_block(&manager),
            "L2 block with payload encoding size equal or greater than max payload size should be sealed"
        );
    }
}
