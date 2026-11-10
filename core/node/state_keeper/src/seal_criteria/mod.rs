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

use zksync_config::configs::chain::SealCriteriaConfig;
use zksync_multivm::{
    interface::{
        DeduplicatedWritesMetrics, FeatureVector, Halt, TransactionExecutionMetrics,
        VmExecutionMetrics,
    },
    vm_latest::TransactionVmExt,
};
use zksync_types::{ProtocolVersionId, Transaction};

pub(crate) use self::criteria::estimate_batch_cycles;
pub use self::{
    conditional_sealer::{ConditionalSealer, NoopSealer, PanicSealer, SequencerSealer},
    io_criteria::IoSealCriteria,
};
use crate::metrics::AGGREGATION_METRICS;

mod conditional_sealer;
pub(super) mod criteria;
pub(super) mod io_criteria;

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
        Halt::FailedBlockTimestampAssertion => "FailedBlockTimestampAssertion",
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
    TooMuchUserL2L1Logs,
    DeploymentNotAllowed,
    RedundantFactoryDeps,
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
            UnexecutableReason::TooMuchUserL2L1Logs => "TooMuchUserL2L1Logs",
            UnexecutableReason::DeploymentNotAllowed => "DeploymentNotAllowed",
            UnexecutableReason::RedundantFactoryDeps => "RedundantFactoryDeps",
        }
    }

    /// Whether this is a property of the *batch* the transaction landed in rather than of the
    /// transaction itself, i.e. whether an otherwise identical batch with more room left could have
    /// executed it.
    ///
    /// Such a failure says nothing about the transaction, so the transaction deserves another batch
    /// rather than a verdict. That is what [`SealResolution::ExcludeAndSeal`] does, and it is why
    /// the caller must consult this before treating a reason as final.
    ///
    /// The distinction only exists because a priority transaction cannot be rejected. An L2
    /// transaction that is dropped can be resubmitted by its sender; an L1->L2 transaction is
    /// already committed on L1, so the only alternatives are to execute it or to stall the whole
    /// priority queue. Stalling the queue over a full batch would be a serious over-reaction, and
    /// `BootloaderOutOfGas` at the tail of a busy batch is an ordinary event, not an incident.
    ///
    /// Everything not listed is treated as final, which is the safe default: the cost of getting it
    /// wrong in that direction is one extra sealed batch, while the other direction re-executes a
    /// transaction that can never succeed, forever.
    pub fn is_batch_relative(&self) -> bool {
        match self {
            // Batch-wide gas: the bootloader's frame is shared by every transaction in the batch, so
            // how much is left depends on what ran before.
            Self::BootloaderOutOfGas
            | Self::NotEnoughGasProvided
            | Self::OutOfGasForBatchTip
            | Self::Halt(Halt::BootloaderOutOfGas)
            | Self::Halt(Halt::NotEnoughGasProvided) => true,
            // Batch-wide VM resources, accumulated across the batch the same way.
            Self::Halt(Halt::MissingInvocationLimitReached)
            | Self::Halt(Halt::FailedToPublishCompressedBytecodes) => true,
            // State of the L2 block being built, not of the transaction.
            Self::Halt(Halt::FailedToSetL2Block(_))
            | Self::Halt(Halt::FailedToAppendTransactionToL2Block(_)) => true,
            // Intrinsic to the transaction, and so is every other halt: validation failures, a gas
            // limit too big to ever schedule, a VM panic. A fresh batch would reproduce all of them.
            Self::Halt(_) => false,
            // Criteria that already compare the transaction *alone* against a whole batch's
            // capacity, and only return `Unexecutable` when it is the sole transaction. A new batch
            // would be no roomier.
            Self::TxEncodingSize
            | Self::LargeEncodingSize
            | Self::PubdataLimit
            | Self::ProofWillFail
            | Self::TooMuchGas
            | Self::TooMuchUserL2L1Logs
            | Self::DeploymentNotAllowed
            | Self::RedundantFactoryDeps => false,
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
            UnexecutableReason::TooMuchUserL2L1Logs => write!(f, "Too much user l2 l1 logs"),
            UnexecutableReason::DeploymentNotAllowed => write!(f, "Deployment not allowed"),
            UnexecutableReason::RedundantFactoryDeps => {
                write!(
                    f,
                    "Too many factory dependencies the transaction does not publish"
                )
            }
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
    pub(super) cumulative_size: usize,
    pub(super) writes_metrics: DeduplicatedWritesMetrics,
    pub(super) gas_remaining: u32,
    /// Airbender cycle-estimator features — per-transaction in `tx_data`, accumulated
    /// over the pending batch in `block_data`.
    pub(super) cycle_features: FeatureVector,
}

impl SealData {
    /// Creates sealing data based on the execution of a `transaction`. Assumes that all writes
    /// performed by the transaction are initial.
    pub(crate) fn for_transaction(
        transaction: &Transaction,
        tx_metrics: &TransactionExecutionMetrics,
    ) -> Self {
        Self {
            execution_metrics: tx_metrics.vm,
            cumulative_size: transaction.bootloader_encoding_size(),
            writes_metrics: tx_metrics.writes,
            gas_remaining: tx_metrics.gas_remaining,
            // The single-tx API/mempool filter has no traced features; the batch seal
            // path fills this from the VM statistics.
            cycle_features: FeatureVector::default(),
        }
    }
}

pub(super) trait SealCriterion: fmt::Debug + Send + Sync + 'static {
    #[allow(clippy::too_many_arguments)]
    fn should_seal(
        &self,
        config: &SealCriteriaConfig,
        tx_count: usize,
        l1_tx_count: usize,
        interop_roots_count: usize,
        block_data: &SealData,
        tx_data: &SealData,
        protocol_version: ProtocolVersionId,
    ) -> SealResolution;

    /// Returns fraction of the criterion's capacity filled in the batch.
    /// If it can't be calculated for the criterion, then it should return `None`.
    fn capacity_filled(
        &self,
        _config: &SealCriteriaConfig,
        _tx_count: usize,
        _l1_tx_count: usize,
        _interop_roots_count: usize,
        _block_data: &SealData,
        _protocol_version: ProtocolVersionId,
    ) -> Option<f64> {
        None
    }

    // We need self here only for rust restrictions for creating an object from trait
    // https://doc.rust-lang.org/reference/items/traits.html#object-safety
    fn prom_criterion_name(&self) -> &'static str;
}
