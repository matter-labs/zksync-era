use thiserror::Error;
use zksync_multivm::interface::ExecutionResult;
use zksync_types::{l2::error::TxCheckError, Address, U256};
use zksync_web3_decl::error::EnrichedClientError;

use crate::execution_sandbox::{SandboxExecutionError, ValidationError};

/// Errors that con occur submitting a transaction or estimating gas for its execution.
#[derive(Debug, Error)]
pub enum SubmitTxError {
    #[error("nonce too high. allowed nonce range: {0} - {1}, actual: {2}")]
    NonceIsTooHigh(u32, u32, u32),
    #[error("nonce too low. allowed nonce range: {0} - {1}, actual: {2}")]
    NonceIsTooLow(u32, u32, u32),
    #[error("insertion of another transaction with the same nonce is in progress")]
    InsertionInProgress,
    #[error("{0}")]
    IncorrectTx(#[from] TxCheckError),
    #[error("insufficient funds for gas + value. balance: {0}, fee: {1}, value: {2}")]
    NotEnoughBalanceForFeeValue(U256, U256, U256),
    #[error("execution reverted{}{}", if.0.is_empty() { "" } else { ": " }, .0)]
    ExecutionReverted(String, Vec<u8>),
    #[error("exceeds block gas limit")]
    GasLimitIsTooBig,
    #[error("{0}")]
    Unexecutable(String),
    #[error("server shutting down")]
    ServerShuttingDown,
    #[error("failed to include transaction in the system. reason: {0}")]
    BootloaderFailure(String),
    #[error("failed to validate the transaction. reason: {0}")]
    ValidationFailed(String),
    #[error("not enough balance to cover the fee. error message: {0}")]
    FailedToChargeFee(String),
    #[error("failed paymaster validation. error message: {0}")]
    PaymasterValidationFailed(String),
    #[error("failed pre-paymaster preparation. error message: {0}")]
    PrePaymasterPreparationFailed(String),
    #[error("invalid sender. can't start a transaction from a non-account")]
    FromIsNotAnAccount,
    #[error("max fee per gas less than block base fee")]
    MaxFeePerGasTooLow,
    #[error("max priority fee per gas higher than max fee per gas")]
    MaxPriorityFeeGreaterThanMaxFee,
    #[error(
    "virtual machine entered unexpected state. please contact developers and provide transaction details \
        that caused this error. Error description: {0}"
    )]
    UnexpectedVMBehavior(String),
    #[error(
        "too many factory dependencies in the transaction. {0} provided, while only {1} allowed"
    )]
    TooManyFactoryDependencies(usize, usize),
    /// IntrinsicGas is returned if the transaction is specified to use less gas
    /// than required to start the invocation.
    #[error("intrinsic gas too low")]
    IntrinsicGas,
    #[error("not enough gas to publish compressed bytecodes")]
    FailedToPublishCompressedBytecodes,
    /// Currently only triggered during gas estimation for L1 and protocol upgrade transactions.
    #[error("integer overflow computing base token amount to mint")]
    MintedAmountOverflow,
    #[error("transaction failed block.timestamp assertion")]
    FailedBlockTimestampAssertion,

    /// Error returned from main node.
    #[error("{0}")]
    ProxyError(#[from] EnrichedClientError),
    /// Catch-all internal error (e.g., database error) that should not be exposed to the caller.
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("contract deployer address {0} is not in the allow list")]
    DeployerNotInAllowList(Address),
}

impl SubmitTxError {
    pub fn prom_error_code(&self) -> &'static str {
        match self {
            Self::NonceIsTooHigh(_, _, _) => "nonce-is-too-high",
            Self::NonceIsTooLow(_, _, _) => "nonce-is-too-low",
            Self::InsertionInProgress => "insertion-in-progress",
            Self::IncorrectTx(_) => "incorrect-tx",
            Self::NotEnoughBalanceForFeeValue(_, _, _) => "not-enough-balance-for-fee",
            Self::ExecutionReverted(_, _) => "execution-reverted",
            Self::GasLimitIsTooBig => "gas-limit-is-too-big",
            Self::Unexecutable(_) => "unexecutable",
            Self::ServerShuttingDown => "shutting-down",
            Self::BootloaderFailure(_) => "bootloader-failure",
            Self::ValidationFailed(_) => "validation-failed",
            Self::FailedToChargeFee(_) => "failed-too-charge-fee",
            Self::PaymasterValidationFailed(_) => "failed-paymaster-validation",
            Self::PrePaymasterPreparationFailed(_) => "failed-prepaymaster-preparation",
            Self::FromIsNotAnAccount => "from-is-not-an-account",
            Self::MaxFeePerGasTooLow => "max-fee-per-gas-too-low",
            Self::MaxPriorityFeeGreaterThanMaxFee => "max-priority-fee-greater-than-max-fee",
            Self::UnexpectedVMBehavior(_) => "unexpected-vm-behavior",
            Self::TooManyFactoryDependencies(_, _) => "too-many-factory-dependencies",
            Self::IntrinsicGas => "intrinsic-gas",
            Self::FailedToPublishCompressedBytecodes => "failed-to-publish-compressed-bytecodes",
            Self::MintedAmountOverflow => "minted-amount-overflow",
            Self::FailedBlockTimestampAssertion => "failed-block-timestamp-assertion",
            Self::ProxyError(_) => "proxy-error",
            Self::Internal(_) => "internal",
            Self::DeployerNotInAllowList(_) => "deployer-not-in-allow-list",
        }
    }

    pub fn data(&self) -> Vec<u8> {
        if let Self::ExecutionReverted(_, data) = self {
            data.clone()
        } else {
            Vec::new()
        }
    }
}

impl From<SandboxExecutionError> for SubmitTxError {
    fn from(err: SandboxExecutionError) -> SubmitTxError {
        match err {
            SandboxExecutionError::Revert(reason, data) => Self::ExecutionReverted(reason, data),
            SandboxExecutionError::BootloaderFailure(reason) => Self::BootloaderFailure(reason),
            SandboxExecutionError::AccountValidationFailed(reason) => {
                Self::ValidationFailed(reason)
            }
            SandboxExecutionError::PaymasterValidationFailed(reason) => {
                Self::PaymasterValidationFailed(reason)
            }
            SandboxExecutionError::PrePaymasterPreparationFailed(reason) => {
                Self::PrePaymasterPreparationFailed(reason)
            }
            SandboxExecutionError::FailedToChargeFee(reason) => Self::FailedToChargeFee(reason),
            SandboxExecutionError::FromIsNotAnAccount => Self::FromIsNotAnAccount,
            SandboxExecutionError::InnerTxError => {
                Self::ExecutionReverted("Bootloader-based tx failed".to_owned(), vec![])
            }
            SandboxExecutionError::UnexpectedVMBehavior(reason) => {
                Self::UnexpectedVMBehavior(reason)
            }
            SandboxExecutionError::FailedToPayForTransaction(reason) => {
                Self::FailedToChargeFee(reason)
            }
            SandboxExecutionError::FailedBlockTimestampAssertion => {
                Self::FailedBlockTimestampAssertion
            }
        }
    }
}

impl From<ValidationError> for SubmitTxError {
    fn from(err: ValidationError) -> Self {
        match err {
            ValidationError::Internal(err) => Self::Internal(err),
            ValidationError::Vm(err) => Self::ValidationFailed(err.to_string()),
        }
    }
}

pub(crate) trait ApiCallResult: Sized {
    fn check_api_call_result(&self) -> Result<(), SubmitTxError>;

    fn into_api_call_result(self) -> Result<Vec<u8>, SubmitTxError>;
}

impl ApiCallResult for ExecutionResult {
    fn check_api_call_result(&self) -> Result<(), SubmitTxError> {
        match self {
            Self::Success { .. } => Ok(()),
            Self::Revert { output } => Err(SubmitTxError::ExecutionReverted(
                output.to_user_friendly_string(),
                output.encoded_data(),
            )),
            Self::Halt { reason } => {
                let output: SandboxExecutionError = reason.clone().into();
                Err(output.into())
            }
        }
    }

    fn into_api_call_result(self) -> Result<Vec<u8>, SubmitTxError> {
        match self {
            Self::Success { output } => Ok(output),
            Self::Revert { output } => Err(SubmitTxError::ExecutionReverted(
                output.to_user_friendly_string(),
                output.encoded_data(),
            )),
            Self::Halt { reason } => {
                let output: SandboxExecutionError = reason.into();
                Err(output.into())
            }
        }
    }
}
