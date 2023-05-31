use crate::api_server::execution_sandbox::SandboxExecutionError;
use thiserror::Error;
use vm::oracles::tracer::ValidationError;
use zksync_types::l2::error::TxCheckError;
use zksync_types::U256;

#[derive(Debug, Error)]
pub enum SubmitTxError {
    #[error("nonce too high. allowed nonce range: {0} - {1}, actual: {2}")]
    NonceIsTooHigh(u32, u32, u32),
    #[error("nonce too low. allowed nonce range: {0} - {1}, actual: {2}")]
    NonceIsTooLow(u32, u32, u32),
    #[error("{0}")]
    IncorrectTx(#[from] TxCheckError),
    #[error("insufficient funds for gas + value. balance: {0}, fee: {1}, value: {2}")]
    NotEnoughBalanceForFeeValue(U256, U256, U256),
    #[error("execution reverted{}{}" , if .0.is_empty() { "" } else { ": " }, .0)]
    ExecutionReverted(String, Vec<u8>),
    #[error("exceeds block gas limit")]
    GasLimitIsTooBig,
    #[error("{0}")]
    Unexecutable(String),
    #[error("too many transactions")]
    RateLimitExceeded,
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
    #[error("pubdata price limit is too low, ensure that the price limit is correct")]
    UnrealisticPubdataPriceLimit,
    #[error(
        "too many factory dependencies in the transaction. {0} provided, while only {1} allowed"
    )]
    TooManyFactoryDependencies(usize, usize),
    #[error("max fee per gas higher than 2^32")]
    FeePerGasTooHigh,
    #[error("max fee per pubdata byte higher than 2^32")]
    FeePerPubdataByteTooHigh,
    /// InsufficientFundsForTransfer is returned if the transaction sender doesn't
    /// have enough funds for transfer.
    #[error("insufficient balance for transfer")]
    InsufficientFundsForTransfer,
    /// IntrinsicGas is returned if the transaction is specified to use less gas
    /// than required to start the invocation.
    #[error("intrinsic gas too low")]
    IntrinsicGas,
    /// Error returned from main node
    #[error("{0}")]
    ProxyError(#[from] zksync_web3_decl::jsonrpsee::core::Error),
}

impl SubmitTxError {
    pub fn grafana_error_code(&self) -> &'static str {
        match self {
            SubmitTxError::NonceIsTooHigh(_, _, _) => "nonce-is-too-high",
            SubmitTxError::NonceIsTooLow(_, _, _) => "nonce-is-too-low",
            SubmitTxError::IncorrectTx(_) => "incorrect-tx",
            SubmitTxError::NotEnoughBalanceForFeeValue(_, _, _) => "not-enough-balance-for-fee",
            SubmitTxError::ExecutionReverted(_, _) => "execution-reverted",
            SubmitTxError::GasLimitIsTooBig => "gas-limit-is-too-big",
            SubmitTxError::Unexecutable(_) => "unexecutable",
            SubmitTxError::RateLimitExceeded => "rate-limit-exceeded",
            SubmitTxError::BootloaderFailure(_) => "bootloader-failure",
            SubmitTxError::ValidationFailed(_) => "validation-failed",
            SubmitTxError::FailedToChargeFee(_) => "failed-too-charge-fee",
            SubmitTxError::PaymasterValidationFailed(_) => "failed-paymaster-validation",
            SubmitTxError::PrePaymasterPreparationFailed(_) => "failed-prepaymaster-preparation",
            SubmitTxError::FromIsNotAnAccount => "from-is-not-an-account",
            SubmitTxError::MaxFeePerGasTooLow => "max-fee-per-gas-too-low",
            SubmitTxError::MaxPriorityFeeGreaterThanMaxFee => {
                "max-priority-fee-greater-than-max-fee"
            }
            SubmitTxError::UnexpectedVMBehavior(_) => "unexpected-vm-behavior",
            SubmitTxError::UnrealisticPubdataPriceLimit => "unrealistic-pubdata-price-limit",
            SubmitTxError::TooManyFactoryDependencies(_, _) => "too-many-factory-dependencies",
            SubmitTxError::FeePerGasTooHigh => "gas-price-limit-too-high",
            SubmitTxError::FeePerPubdataByteTooHigh => "pubdata-price-limit-too-high",
            SubmitTxError::InsufficientFundsForTransfer => "insufficient-funds-for-transfer",
            SubmitTxError::IntrinsicGas => "intrinsic-gas",
            SubmitTxError::ProxyError(_) => "proxy-error",
        }
    }

    pub fn data(&self) -> Vec<u8> {
        if let SubmitTxError::ExecutionReverted(_, data) = self {
            data.clone()
        } else {
            Vec::new()
        }
    }
}

impl From<SandboxExecutionError> for SubmitTxError {
    fn from(err: SandboxExecutionError) -> SubmitTxError {
        match err {
            SandboxExecutionError::Revert(reason, data) => {
                SubmitTxError::ExecutionReverted(reason, data)
            }
            SandboxExecutionError::BootloaderFailure(reason) => {
                SubmitTxError::BootloaderFailure(reason)
            }
            SandboxExecutionError::AccountValidationFailed(reason) => {
                SubmitTxError::ValidationFailed(reason)
            }
            SandboxExecutionError::PaymasterValidationFailed(reason) => {
                SubmitTxError::PaymasterValidationFailed(reason)
            }
            SandboxExecutionError::PrePaymasterPreparationFailed(reason) => {
                SubmitTxError::PrePaymasterPreparationFailed(reason)
            }
            SandboxExecutionError::FailedToChargeFee(reason) => {
                SubmitTxError::FailedToChargeFee(reason)
            }
            SandboxExecutionError::FromIsNotAnAccount => SubmitTxError::FromIsNotAnAccount,
            SandboxExecutionError::InnerTxError => {
                SubmitTxError::ExecutionReverted("Bootloader-based tx failed".to_owned(), vec![])
            }
            SandboxExecutionError::UnexpectedVMBehavior(reason) => {
                SubmitTxError::UnexpectedVMBehavior(reason)
            }
            SandboxExecutionError::FailedToPayForTransaction(reason) => {
                SubmitTxError::FailedToChargeFee(reason)
            }
            SandboxExecutionError::Unexecutable(reason) => SubmitTxError::Unexecutable(reason),
        }
    }
}

impl From<ValidationError> for SubmitTxError {
    fn from(err: ValidationError) -> Self {
        Self::ValidationFailed(err.to_string())
    }
}
