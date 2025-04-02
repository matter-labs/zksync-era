use zksync_multivm::interface::{Halt, TxRevertReason};

#[derive(Debug)]
pub(crate) enum SandboxExecutionError {
    AccountValidationFailed(String),
    FailedToChargeFee(String),
    PaymasterValidationFailed(String),
    PrePaymasterPreparationFailed(String),
    FromIsNotAnAccount,
    BootloaderFailure(String),
    Revert(String, Vec<u8>),
    FailedToPayForTransaction(String),
    InnerTxError,
    UnexpectedVMBehavior(String),
    FailedBlockTimestampAssertion,
}

impl From<Halt> for SandboxExecutionError {
    fn from(value: Halt) -> Self {
        match value {
            Halt::FailedToChargeFee(reason) => Self::FailedToChargeFee(reason.to_string()),
            Halt::FromIsNotAnAccount => Self::FromIsNotAnAccount,
            Halt::InnerTxError => Self::InnerTxError,
            Halt::Unknown(reason) => Self::BootloaderFailure(reason.to_string()),
            Halt::ValidationFailed(reason) => Self::AccountValidationFailed(reason.to_string()),
            Halt::PaymasterValidationFailed(reason) => {
                Self::PaymasterValidationFailed(reason.to_string())
            }
            Halt::PrePaymasterPreparationFailed(reason) => {
                Self::PrePaymasterPreparationFailed(reason.to_string())
            }
            Halt::UnexpectedVMBehavior(reason) => Self::UnexpectedVMBehavior(reason),
            Halt::BootloaderOutOfGas => {
                Self::UnexpectedVMBehavior("bootloader is out of gas".to_string())
            }
            Halt::NotEnoughGasProvided => Self::UnexpectedVMBehavior(
                "The bootloader did not contain enough gas to execute the transaction".to_string(),
            ),
            revert_reason @ Halt::FailedToMarkFactoryDependencies(_) => {
                Self::Revert(revert_reason.to_string(), vec![])
            }
            Halt::PayForTxFailed(reason) => Self::FailedToPayForTransaction(reason.to_string()),
            Halt::TooBigGasLimit => Self::Revert(Halt::TooBigGasLimit.to_string(), vec![]),
            Halt::MissingInvocationLimitReached => Self::InnerTxError,
            Halt::VMPanic => Self::UnexpectedVMBehavior("VM panic".to_string()),
            Halt::FailedToSetL2Block(reason) => SandboxExecutionError::Revert(reason, vec![]),
            Halt::FailedToAppendTransactionToL2Block(reason) => {
                SandboxExecutionError::Revert(reason, vec![])
            }
            Halt::TracerCustom(reason) => SandboxExecutionError::Revert(reason, vec![]),
            Halt::ValidationOutOfGas => Self::AccountValidationFailed(
                "The validation of the transaction ran out of gas".to_string(),
            ),
            Halt::FailedToPublishCompressedBytecodes => {
                Self::UnexpectedVMBehavior("Failed to publish compressed bytecodes".to_string())
            }
            Halt::FailedBlockTimestampAssertion => Self::FailedBlockTimestampAssertion,
        }
    }
}

impl From<TxRevertReason> for SandboxExecutionError {
    fn from(reason: TxRevertReason) -> Self {
        match reason {
            TxRevertReason::TxReverted(reason) => SandboxExecutionError::Revert(
                reason.to_user_friendly_string(),
                reason.encoded_data(),
            ),
            TxRevertReason::Halt(halt) => SandboxExecutionError::from(halt),
        }
    }
}
