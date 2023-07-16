use thiserror::Error;

use vm::TxRevertReason;

#[derive(Debug, Error)]
pub(crate) enum SandboxExecutionError {
    #[error("Account validation failed: {0}")]
    AccountValidationFailed(String),
    #[error("Failed to charge fee: {0}")]
    FailedToChargeFee(String),
    #[error("Paymaster validation failed: {0}")]
    PaymasterValidationFailed(String),
    #[error("Pre-paymaster preparation failed: {0}")]
    PrePaymasterPreparationFailed(String),
    #[error("From is not an account")]
    FromIsNotAnAccount,
    #[error("Bootloader failure: {0}")]
    BootloaderFailure(String),
    #[error("Revert: {0}")]
    Revert(String, Vec<u8>),
    #[error("Failed to pay for the transaction: {0}")]
    FailedToPayForTransaction(String),
    #[error("Bootloader-based tx failed")]
    InnerTxError,
    #[error(
    "Virtual machine entered unexpected state. Please contact developers and provide transaction details \
        that caused this error. Error description: {0}"
    )]
    UnexpectedVMBehavior(String),
    #[error("Transaction is unexecutable. Reason: {0}")]
    Unexecutable(String),
}

impl From<TxRevertReason> for SandboxExecutionError {
    fn from(reason: TxRevertReason) -> Self {
        match reason {
            TxRevertReason::EthCall(reason) => SandboxExecutionError::Revert(
                reason.to_user_friendly_string(),
                reason.encoded_data(),
            ),
            TxRevertReason::TxReverted(reason) => SandboxExecutionError::Revert(
                reason.to_user_friendly_string(),
                reason.encoded_data(),
            ),
            TxRevertReason::FailedToChargeFee(reason) => {
                SandboxExecutionError::FailedToChargeFee(reason.to_string())
            }
            TxRevertReason::FromIsNotAnAccount => SandboxExecutionError::FromIsNotAnAccount,
            TxRevertReason::InnerTxError => SandboxExecutionError::InnerTxError,
            TxRevertReason::Unknown(reason) => {
                SandboxExecutionError::BootloaderFailure(reason.to_string())
            }
            TxRevertReason::ValidationFailed(reason) => {
                SandboxExecutionError::AccountValidationFailed(reason.to_string())
            }
            TxRevertReason::PaymasterValidationFailed(reason) => {
                SandboxExecutionError::PaymasterValidationFailed(reason.to_string())
            }
            TxRevertReason::PrePaymasterPreparationFailed(reason) => {
                SandboxExecutionError::PrePaymasterPreparationFailed(reason.to_string())
            }
            TxRevertReason::UnexpectedVMBehavior(reason) => {
                SandboxExecutionError::UnexpectedVMBehavior(reason)
            }
            TxRevertReason::BootloaderOutOfGas => {
                SandboxExecutionError::UnexpectedVMBehavior("bootloader is out of gas".to_string())
            }
            TxRevertReason::NotEnoughGasProvided => SandboxExecutionError::UnexpectedVMBehavior(
                "The bootloader did not contain enough gas to execute the transaction".to_string(),
            ),
            revert_reason @ TxRevertReason::FailedToMarkFactoryDependencies(_) => {
                SandboxExecutionError::Revert(revert_reason.to_string(), vec![])
            }
            TxRevertReason::PayForTxFailed(reason) => {
                SandboxExecutionError::FailedToPayForTransaction(reason.to_string())
            }
            TxRevertReason::TooBigGasLimit => {
                SandboxExecutionError::Revert(TxRevertReason::TooBigGasLimit.to_string(), vec![])
            }
            TxRevertReason::MissingInvocationLimitReached => SandboxExecutionError::InnerTxError,
        }
    }
}
