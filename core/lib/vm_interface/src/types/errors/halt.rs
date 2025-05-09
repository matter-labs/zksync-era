use std::fmt;

use super::VmRevertReason;

/// Structure for non-contract errors from the Virtual Machine (EVM).
///
/// Differentiates VM-specific issues from contract-related errors.
#[derive(Debug, Clone, PartialEq)]
pub enum Halt {
    // Can only be returned in `VerifyAndExecute`
    ValidationFailed(VmRevertReason),
    PaymasterValidationFailed(VmRevertReason),
    PrePaymasterPreparationFailed(VmRevertReason),
    PayForTxFailed(VmRevertReason),
    FailedToMarkFactoryDependencies(VmRevertReason),
    FailedToChargeFee(VmRevertReason),
    // Emitted when trying to call a transaction from an account that has not
    // been deployed as an account (i.e. the `from` is just a contract).
    // Can only be returned in `VerifyAndExecute`
    FromIsNotAnAccount,
    // Currently cannot be returned. Should be removed when refactoring errors.
    InnerTxError,
    Unknown(VmRevertReason),
    // Temporarily used instead of panics to provide better experience for developers:
    // their transaction would simply be rejected and they'll be able to provide
    // information about the cause to us.
    UnexpectedVMBehavior(String),
    // Bootloader is out of gas.
    BootloaderOutOfGas,
    // Validation step is out of gas
    ValidationOutOfGas,
    // Transaction has a too big gas limit and will not be executed by the server.
    TooBigGasLimit,
    // The bootloader did not have enough gas to start the transaction in the first place
    NotEnoughGasProvided,
    // The tx consumes too much missing invocations to memory
    MissingInvocationLimitReached,
    // Failed to set information about the L2 block
    FailedToSetL2Block(String),
    // Failed to publish information about the batch and the L2 block onto L1
    FailedToAppendTransactionToL2Block(String),
    VMPanic,
    TracerCustom(String),
    FailedToPublishCompressedBytecodes,
    FailedBlockTimestampAssertion,
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Halt::ValidationFailed(reason) => {
                write!(f, "Account validation error: {}", reason)
            }
            Halt::FailedToChargeFee(reason) => {
                write!(f, "Failed to charge fee: {}", reason)
            }
            // Emitted when trying to call a transaction from an account that has no
            // been deployed as an account (i.e. the `from` is just a contract).
            Halt::FromIsNotAnAccount => write!(f, "Sender is not an account"),
            Halt::InnerTxError => write!(f, "Bootloader-based tx failed"),
            Halt::PaymasterValidationFailed(reason) => {
                write!(f, "Paymaster validation error: {}", reason)
            }
            Halt::PrePaymasterPreparationFailed(reason) => {
                write!(f, "Pre-paymaster preparation error: {}", reason)
            }
            Halt::Unknown(reason) => write!(f, "Unknown reason: {}", reason),
            Halt::UnexpectedVMBehavior(problem) => {
                write!(f,
                       "virtual machine entered unexpected state. Please contact developers and provide transaction details \
                    that caused this error. Error description: {problem}"
                )
            }
            Halt::BootloaderOutOfGas => write!(f, "Bootloader out of gas"),
            Halt::NotEnoughGasProvided => write!(
                f,
                "Bootloader did not have enough gas to start the transaction"
            ),
            Halt::FailedToMarkFactoryDependencies(reason) => {
                write!(f, "Failed to mark factory dependencies: {}", reason)
            }
            Halt::PayForTxFailed(reason) => {
                write!(f, "Failed to pay for the transaction: {}", reason)
            }
            Halt::TooBigGasLimit => {
                write!(
                    f,
                    "Transaction has a too big ergs limit and will not be executed by the server"
                )
            }
            Halt::MissingInvocationLimitReached => {
                write!(f, "Tx produced too much cold storage accesses")
            }
            Halt::VMPanic => {
                write!(f, "VM panicked")
            }
            Halt::FailedToSetL2Block(reason) => {
                write!(
                    f,
                    "Failed to set information about the L2 block: {}",
                    reason
                )
            }
            Halt::FailedToAppendTransactionToL2Block(reason) => {
                write!(
                    f,
                    "Failed to append the transaction to the current L2 block: {}",
                    reason
                )
            }
            Halt::TracerCustom(reason) => {
                write!(f, "Tracer aborted execution: {}", reason)
            }
            Halt::ValidationOutOfGas => {
                write!(f, "Validation run out of gas")
            }
            Halt::FailedToPublishCompressedBytecodes => {
                write!(f, "Failed to publish compressed bytecodes")
            }
            Halt::FailedBlockTimestampAssertion => {
                write!(f, "Transaction failed block.timestamp assertion")
            }
        }
    }
}
