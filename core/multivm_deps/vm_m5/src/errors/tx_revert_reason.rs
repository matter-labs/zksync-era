use std::{convert::TryFrom, fmt::Display};

use super::{BootloaderErrorCode, VmRevertReason};

// Note that currently only EthCall transactions have valid Revert Reason.
// Same transaction executed in bootloader will just have `InnerTxError`.
// Reasons why the transaction executed inside the bootloader could fail.
#[derive(Debug, Clone, PartialEq)]
pub enum TxRevertReason {
    // Can only be returned in EthCall execution mode (=ExecuteOnly)
    EthCall(VmRevertReason),
    // Returned when the execution of an L2 transaction has failed
    TxReverted(VmRevertReason),
    // Can only be returned in VerifyAndExecute
    ValidationFailed(VmRevertReason),
    PaymasterValidationFailed(VmRevertReason),
    PrePaymasterPreparationFailed(VmRevertReason),
    PayForTxFailed(VmRevertReason),
    FailedToMarkFactoryDependencies(VmRevertReason),
    FailedToChargeFee(VmRevertReason),
    // Emitted when trying to call a transaction from an account that has not
    // been deployed as an account (i.e. the `from` is just a contract).
    // Can only be returned in VerifyAndExecute
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
    // Transaction has a too big gas limit and will not be executed by the server.
    TooBigGasLimit,
    // The bootloader did not have enough gas to start the transaction in the first place
    NotEnoughGasProvided,
}

impl TxRevertReason {
    pub fn parse_error(bytes: &[u8]) -> Self {
        // The first 32 bytes should correspond with error code.
        // If the error is smaller than that, we will use a standardized bootloader error.
        if bytes.is_empty() {
            return Self::UnexpectedVMBehavior("Bootloader returned an empty error".to_string());
        }

        let (error_code, error_msg) = bytes.split_at(1);
        let revert_reason = match VmRevertReason::try_from(error_msg) {
            Ok(reason) => reason,
            Err(_) => {
                let function_selector = if error_msg.len() >= 4 {
                    error_msg[0..4].to_vec()
                } else {
                    error_msg.to_vec()
                };

                let data = if error_msg.len() > 4 {
                    error_msg[4..].to_vec()
                } else {
                    vec![]
                };

                VmRevertReason::Unknown {
                    function_selector,
                    data,
                }
            }
        };

        // `error_code` is a big-endian number, so we can safely take the first byte of it.
        match BootloaderErrorCode::from(error_code[0]) {
            BootloaderErrorCode::EthCall => Self::EthCall(revert_reason),
            BootloaderErrorCode::AccountTxValidationFailed => Self::ValidationFailed(revert_reason),
            BootloaderErrorCode::FailedToChargeFee => Self::FailedToChargeFee(revert_reason),
            BootloaderErrorCode::FromIsNotAnAccount => Self::FromIsNotAnAccount,
            BootloaderErrorCode::FailedToCheckAccount => Self::ValidationFailed(VmRevertReason::General {
                msg: "Failed to check if `from` is an account. Most likely not enough gas provided".to_string()
            }),
            BootloaderErrorCode::UnacceptableGasPrice => Self::UnexpectedVMBehavior(
                "The operator included transaction with an unacceptable gas price".to_owned(),
            ),
            BootloaderErrorCode::PrePaymasterPreparationFailed => {
                Self::PrePaymasterPreparationFailed(revert_reason)
            }
            BootloaderErrorCode::PaymasterValidationFailed => {
                Self::PaymasterValidationFailed(revert_reason)
            }
            BootloaderErrorCode::FailedToSendFeesToTheOperator => {
                Self::UnexpectedVMBehavior("FailedToSendFeesToTheOperator".to_owned())
            }
            BootloaderErrorCode::FailedToSetPrevBlockHash => {
                panic!(
                    "The bootloader failed to set previous block hash. Reason: {}",
                    revert_reason
                )
            }
            BootloaderErrorCode::UnacceptablePubdataPrice => {
                Self::UnexpectedVMBehavior("UnacceptablePubdataPrice".to_owned())
            }
            // This is different from AccountTxValidationFailed error in a way that it means that
            // the error was not produced by the account itself, but for some other unknown reason (most likely not enough gas)
            BootloaderErrorCode::TxValidationError => Self::ValidationFailed(revert_reason),
            // Note, that `InnerTxError` is derived only after the actual tx execution, so
            // it is not parsed here. Unknown error means that bootloader failed by a reason
            // that was not specified by the protocol:
            BootloaderErrorCode::MaxPriorityFeeGreaterThanMaxFee => {
                Self::UnexpectedVMBehavior("Max priority fee greater than max fee".to_owned())
            }
            BootloaderErrorCode::PaymasterReturnedInvalidContext => {
                Self::PaymasterValidationFailed(VmRevertReason::General {
                    msg: String::from("Paymaster returned invalid context"),
                })
            }
            BootloaderErrorCode::PaymasterContextIsTooLong => {
                Self::PaymasterValidationFailed(VmRevertReason::General {
                    msg: String::from("Paymaster returned context that is too long"),
                })
            }
            BootloaderErrorCode::AssertionError => {
                Self::UnexpectedVMBehavior(format!("Assertion error: {}", revert_reason))
            }
            BootloaderErrorCode::BaseFeeGreaterThanMaxFeePerGas => Self::UnexpectedVMBehavior(
                "Block.basefee is greater than max fee per gas".to_owned(),
            ),
            BootloaderErrorCode::PayForTxFailed => {
                Self::PayForTxFailed(revert_reason)
            },
            BootloaderErrorCode::FailedToMarkFactoryDeps => {
                let msg = if let VmRevertReason::General { msg } = revert_reason {
                    msg
                } else {
                    String::from("Most likely not enough gas provided")
                };
                Self::FailedToMarkFactoryDependencies(VmRevertReason::General {
                    msg
                })
            },
            BootloaderErrorCode::TxValidationOutOfGas => {
                Self::ValidationFailed(VmRevertReason::General { msg: String::from("Not enough gas for transaction validation") })
            },
            BootloaderErrorCode::NotEnoughGasProvided => {
                Self::NotEnoughGasProvided
            },
            BootloaderErrorCode::AccountReturnedInvalidMagic => {
                Self::ValidationFailed(VmRevertReason::General { msg: String::from("Account validation returned invalid magic value. Most often this means that the signature is incorrect") })
            },
            BootloaderErrorCode::PaymasterReturnedInvalidMagic => {
                Self::ValidationFailed(VmRevertReason::General { msg: String::from("Paymaster validation returned invalid magic value. Please refer to the documentation of the paymaster for more details") })
            }
            BootloaderErrorCode::Unknown => Self::UnexpectedVMBehavior(format!(
                "Unsupported error code: {}. Revert reason: {}",
                error_code[0], revert_reason
            )),
        }
    }
}

impl Display for TxRevertReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            // EthCall reason is usually returned unchanged.
            TxRevertReason::EthCall(reason) => write!(f, "{}", reason),
            TxRevertReason::TxReverted(reason) => write!(f, "{}", reason),
            TxRevertReason::ValidationFailed(reason) => {
                write!(f, "Account validation error: {}", reason)
            }
            TxRevertReason::FailedToChargeFee(reason) => {
                write!(f, "Failed to charge fee: {}", reason)
            }
            // Emitted when trying to call a transaction from an account that has no
            // been deployed as an account (i.e. the `from` is just a contract).
            TxRevertReason::FromIsNotAnAccount => write!(f, "Sender is not an account"),
            TxRevertReason::InnerTxError => write!(f, "Bootloader-based tx failed"),
            TxRevertReason::PaymasterValidationFailed(reason) => {
                write!(f, "Paymaster validation error: {}", reason)
            }
            TxRevertReason::PrePaymasterPreparationFailed(reason) => {
                write!(f, "Pre-paymaster preparation error: {}", reason)
            }
            TxRevertReason::Unknown(reason) => write!(f, "Unknown reason: {}", reason),
            TxRevertReason::UnexpectedVMBehavior(problem) => {
                write!(f,
                    "virtual machine entered unexpected state. Please contact developers and provide transaction details \
                    that caused this error. Error description: {problem}" 
                )
            }
            TxRevertReason::BootloaderOutOfGas => write!(f, "Bootloader out of gas"),
            TxRevertReason::NotEnoughGasProvided => write!(
                f,
                "Bootloader did not have enough gas to start the transaction"
            ),
            TxRevertReason::FailedToMarkFactoryDependencies(reason) => {
                write!(f, "Failed to mark factory dependencies: {}", reason)
            }
            TxRevertReason::PayForTxFailed(reason) => {
                write!(f, "Failed to pay for the transaction: {}", reason)
            }
            TxRevertReason::TooBigGasLimit => {
                write!(
                    f,
                    "Transaction has a too big ergs limit and will not be executed by the server"
                )
            }
        }
    }
}
