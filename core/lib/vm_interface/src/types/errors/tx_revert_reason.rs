use std::fmt;

use super::{halt::Halt, BootloaderErrorCode, VmRevertReason};

#[derive(Debug, Clone, PartialEq)]
pub enum TxRevertReason {
    /// Returned when the execution of an L2 transaction or a call has failed.
    TxReverted(VmRevertReason),
    /// Returned when some validation has failed or some internal errors.
    Halt(Halt),
}

impl TxRevertReason {
    pub fn parse_error(bytes: &[u8]) -> Self {
        // The first 32 bytes should correspond with error code.
        // If the error is smaller than that, we will use a standardized bootloader error.
        if bytes.is_empty() {
            return Self::Halt(Halt::UnexpectedVMBehavior(
                "Bootloader returned an empty error".to_string(),
            ));
        }

        let (error_code, error_msg) = bytes.split_at(1);
        let revert_reason = VmRevertReason::from(error_msg);

        // `error_code` is a big-endian number, so we can safely take the first byte of it.
        match BootloaderErrorCode::from(error_code[0]) {
            BootloaderErrorCode::EthCall => Self::TxReverted(revert_reason),
            BootloaderErrorCode::AccountTxValidationFailed => Self::Halt(Halt::ValidationFailed(revert_reason)),
            BootloaderErrorCode::FailedToChargeFee => Self::Halt(Halt::FailedToChargeFee(revert_reason)),
            BootloaderErrorCode::FromIsNotAnAccount => Self::Halt(Halt::FromIsNotAnAccount),
            BootloaderErrorCode::FailedToCheckAccount => Self::Halt(Halt::ValidationFailed(VmRevertReason::General {
                msg: "Failed to check if `from` is an account. Most likely not enough gas provided".to_string(),
                data: vec![],
            })),
            BootloaderErrorCode::UnacceptableGasPrice => Self::Halt(Halt::UnexpectedVMBehavior(
                "The operator included transaction with an unacceptable gas price".to_owned(),
            )),
            BootloaderErrorCode::PrePaymasterPreparationFailed => {
                Self::Halt(Halt::PrePaymasterPreparationFailed(revert_reason))
            }
            BootloaderErrorCode::PaymasterValidationFailed => {
                Self::Halt(Halt::PaymasterValidationFailed(revert_reason))
            }
            BootloaderErrorCode::FailedToSendFeesToTheOperator => {
                Self::Halt(Halt::UnexpectedVMBehavior("FailedToSendFeesToTheOperator".to_owned()))
            }
            BootloaderErrorCode::FailedToSetPrevBlockHash => {
                panic!(
                    "The bootloader failed to set previous block hash. Reason: {}",
                    revert_reason
                )
            }
            BootloaderErrorCode::UnacceptablePubdataPrice => {
                Self::Halt(Halt::UnexpectedVMBehavior("UnacceptablePubdataPrice".to_owned()))
            }
            // This is different from `AccountTxValidationFailed` error in a way that it means that
            // the error was not produced by the account itself, but for some other unknown reason (most likely not enough gas)
            BootloaderErrorCode::TxValidationError => Self::Halt(Halt::ValidationFailed(revert_reason)),
            // Note, that `InnerTxError` is derived only after the actual tx execution, so
            // it is not parsed here. Unknown error means that bootloader failed by a reason
            // that was not specified by the protocol:
            BootloaderErrorCode::MaxPriorityFeeGreaterThanMaxFee => {
                Self::Halt(Halt::UnexpectedVMBehavior("Max priority fee greater than max fee".to_owned()))
            }
            BootloaderErrorCode::PaymasterReturnedInvalidContext => {
                Self::Halt(Halt::PaymasterValidationFailed(VmRevertReason::General {
                    msg: String::from("Paymaster returned invalid context"),
                    data: vec![],
                }))
            }
            BootloaderErrorCode::PaymasterContextIsTooLong => {
                Self::Halt(Halt::PaymasterValidationFailed(VmRevertReason::General {
                    msg: String::from("Paymaster returned context that is too long"),
                    data: vec![],
                }))
            }
            BootloaderErrorCode::AssertionError => {
                Self::Halt(Halt::UnexpectedVMBehavior(format!("Assertion error: {}", revert_reason)))
            }
            BootloaderErrorCode::BaseFeeGreaterThanMaxFeePerGas => Self::Halt(Halt::UnexpectedVMBehavior(
                "Block.basefee is greater than max fee per gas".to_owned(),
            )),
            BootloaderErrorCode::PayForTxFailed => {
                Self::Halt(Halt::PayForTxFailed(revert_reason))
            },
            BootloaderErrorCode::FailedToMarkFactoryDeps => {
                let (msg, data) = if let VmRevertReason::General { msg , data} = revert_reason {
                    (msg, data)
                } else {
                    (String::from("Most likely not enough gas provided"), vec![])
                };
                Self::Halt(Halt::FailedToMarkFactoryDependencies(VmRevertReason::General {
                    msg, data
                }))
            },
            BootloaderErrorCode::TxValidationOutOfGas => {
                Self::Halt(Halt::ValidationFailed(VmRevertReason::General { msg: String::from("Not enough gas for transaction validation"), data: vec![] }))
            },
            BootloaderErrorCode::NotEnoughGasProvided => {
                Self::Halt(Halt::NotEnoughGasProvided)
            },
            BootloaderErrorCode::AccountReturnedInvalidMagic => {
                Self::Halt(Halt::ValidationFailed(VmRevertReason::General { msg: String::from("Account validation returned invalid magic value. Most often this means that the signature is incorrect"), data: vec![] }))
            },
            BootloaderErrorCode::PaymasterReturnedInvalidMagic => {
                Self::Halt(Halt::ValidationFailed(VmRevertReason::General { msg: String::from("Paymaster validation returned invalid magic value. Please refer to the documentation of the paymaster for more details"), data: vec![] }))
            }
            BootloaderErrorCode::L1MessengerLogSendingFailed => {
                Self::Halt(Halt::UnexpectedVMBehavior(format!("Failed to send log via L1Messenger for: {}", revert_reason)))
            },
            BootloaderErrorCode::L1MessengerPublishingFailed => {
                Self::Halt(Halt::UnexpectedVMBehavior(format!("Failed to publish pubdata via L1Messenger for: {}", revert_reason)))
            },
            BootloaderErrorCode::FailedToCallSystemContext => {
                Self::Halt(Halt::UnexpectedVMBehavior(format!("Failed to call system context contract: {}", revert_reason)))
            },
            BootloaderErrorCode::Unknown => Self::Halt(Halt::UnexpectedVMBehavior(format!(
                "Unsupported error code: {}. Revert reason: {}",
                error_code[0], revert_reason
            ))),
            BootloaderErrorCode::MintEtherFailed => Self::Halt(Halt::UnexpectedVMBehavior(format!("Failed to mint ether: {}", revert_reason))),
            BootloaderErrorCode::FailedToAppendTransactionToL2Block => {
                Self::Halt(Halt::FailedToAppendTransactionToL2Block(format!("Failed to append transaction to L2 block: {}", revert_reason)))
            }
            BootloaderErrorCode::FailedToSetL2Block => {
                Self::Halt(Halt::FailedToSetL2Block(format!("{}", revert_reason)))

            }
            BootloaderErrorCode::FailedToPublishTimestampDataToL1 => {
                Self::Halt(Halt::UnexpectedVMBehavior(format!("Failed to publish timestamp data to L1: {}", revert_reason)))
            }
            BootloaderErrorCode::FailedToProcessEip7702Delegations => {
                Self::Halt(Halt::ValidationFailed(VmRevertReason::General { msg: String::from("Failed to process EIP-7702 authorization list"), data: vec![] }))
            }
        }
    }
}

impl fmt::Display for TxRevertReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            TxRevertReason::TxReverted(reason) => write!(f, "{}", reason),
            TxRevertReason::Halt(reason) => write!(f, "{}", reason),
        }
    }
}
