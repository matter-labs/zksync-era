use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<crate::vm_m5::TxRevertReason> for crate::interface::TxRevertReason {
    fn glue_from(value: crate::vm_m5::TxRevertReason) -> Self {
        match value {
            crate::vm_m5::TxRevertReason::EthCall(err) => Self::TxReverted(err.glue_into()),
            crate::vm_m5::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            crate::vm_m5::TxRevertReason::ValidationFailed(err) => {
                Self::Halt(crate::interface::Halt::ValidationFailed(err.glue_into()))
            }
            crate::vm_m5::TxRevertReason::PaymasterValidationFailed(err) => Self::Halt(
                crate::interface::Halt::PaymasterValidationFailed(err.glue_into()),
            ),
            crate::vm_m5::TxRevertReason::PrePaymasterPreparationFailed(err) => Self::Halt(
                crate::interface::Halt::PrePaymasterPreparationFailed(err.glue_into()),
            ),
            crate::vm_m5::TxRevertReason::PayForTxFailed(err) => {
                Self::Halt(crate::interface::Halt::PayForTxFailed(err.glue_into()))
            }
            crate::vm_m5::TxRevertReason::FailedToMarkFactoryDependencies(err) => Self::Halt(
                crate::interface::Halt::FailedToMarkFactoryDependencies(err.glue_into()),
            ),
            crate::vm_m5::TxRevertReason::FailedToChargeFee(err) => {
                Self::Halt(crate::interface::Halt::FailedToChargeFee(err.glue_into()))
            }
            crate::vm_m5::TxRevertReason::FromIsNotAnAccount => {
                Self::Halt(crate::interface::Halt::FromIsNotAnAccount)
            }
            crate::vm_m5::TxRevertReason::InnerTxError => {
                Self::Halt(crate::interface::Halt::InnerTxError)
            }
            crate::vm_m5::TxRevertReason::Unknown(err) => {
                Self::Halt(crate::interface::Halt::Unknown(err.glue_into()))
            }
            crate::vm_m5::TxRevertReason::UnexpectedVMBehavior(err) => {
                Self::Halt(crate::interface::Halt::UnexpectedVMBehavior(err))
            }
            crate::vm_m5::TxRevertReason::BootloaderOutOfGas => {
                Self::Halt(crate::interface::Halt::BootloaderOutOfGas)
            }
            crate::vm_m5::TxRevertReason::TooBigGasLimit => {
                Self::Halt(crate::interface::Halt::TooBigGasLimit)
            }
            crate::vm_m5::TxRevertReason::NotEnoughGasProvided => {
                Self::Halt(crate::interface::Halt::NotEnoughGasProvided)
            }
        }
    }
}

impl GlueFrom<crate::vm_m6::TxRevertReason> for crate::interface::TxRevertReason {
    fn glue_from(value: crate::vm_m6::TxRevertReason) -> Self {
        match value {
            crate::vm_m6::TxRevertReason::EthCall(err) => Self::TxReverted(err.glue_into()),
            crate::vm_m6::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            crate::vm_m6::TxRevertReason::ValidationFailed(err) => {
                Self::Halt(crate::interface::Halt::ValidationFailed(err.glue_into()))
            }
            crate::vm_m6::TxRevertReason::PaymasterValidationFailed(err) => Self::Halt(
                crate::interface::Halt::PaymasterValidationFailed(err.glue_into()),
            ),
            crate::vm_m6::TxRevertReason::PrePaymasterPreparationFailed(err) => Self::Halt(
                crate::interface::Halt::PrePaymasterPreparationFailed(err.glue_into()),
            ),
            crate::vm_m6::TxRevertReason::PayForTxFailed(err) => {
                Self::Halt(crate::interface::Halt::PayForTxFailed(err.glue_into()))
            }
            crate::vm_m6::TxRevertReason::FailedToMarkFactoryDependencies(err) => Self::Halt(
                crate::interface::Halt::FailedToMarkFactoryDependencies(err.glue_into()),
            ),
            crate::vm_m6::TxRevertReason::FailedToChargeFee(err) => {
                Self::Halt(crate::interface::Halt::FailedToChargeFee(err.glue_into()))
            }
            crate::vm_m6::TxRevertReason::FromIsNotAnAccount => {
                Self::Halt(crate::interface::Halt::FromIsNotAnAccount)
            }
            crate::vm_m6::TxRevertReason::InnerTxError => {
                Self::Halt(crate::interface::Halt::InnerTxError)
            }
            crate::vm_m6::TxRevertReason::Unknown(err) => {
                Self::Halt(crate::interface::Halt::Unknown(err.glue_into()))
            }
            crate::vm_m6::TxRevertReason::UnexpectedVMBehavior(err) => {
                Self::Halt(crate::interface::Halt::UnexpectedVMBehavior(err))
            }
            crate::vm_m6::TxRevertReason::BootloaderOutOfGas => {
                Self::Halt(crate::interface::Halt::BootloaderOutOfGas)
            }
            crate::vm_m6::TxRevertReason::TooBigGasLimit => {
                Self::Halt(crate::interface::Halt::TooBigGasLimit)
            }
            crate::vm_m6::TxRevertReason::NotEnoughGasProvided => {
                Self::Halt(crate::interface::Halt::NotEnoughGasProvided)
            }
            crate::vm_m6::TxRevertReason::MissingInvocationLimitReached => {
                Self::Halt(crate::interface::Halt::MissingInvocationLimitReached)
            }
        }
    }
}

impl GlueFrom<crate::vm_1_3_2::TxRevertReason> for crate::interface::TxRevertReason {
    fn glue_from(value: crate::vm_1_3_2::TxRevertReason) -> Self {
        match value {
            crate::vm_1_3_2::TxRevertReason::EthCall(err) => Self::TxReverted(err.glue_into()),
            crate::vm_1_3_2::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            crate::vm_1_3_2::TxRevertReason::ValidationFailed(err) => {
                Self::Halt(crate::interface::Halt::ValidationFailed(err.glue_into()))
            }
            crate::vm_1_3_2::TxRevertReason::PaymasterValidationFailed(err) => Self::Halt(
                crate::interface::Halt::PaymasterValidationFailed(err.glue_into()),
            ),
            crate::vm_1_3_2::TxRevertReason::PrePaymasterPreparationFailed(err) => Self::Halt(
                crate::interface::Halt::PrePaymasterPreparationFailed(err.glue_into()),
            ),
            crate::vm_1_3_2::TxRevertReason::PayForTxFailed(err) => {
                Self::Halt(crate::interface::Halt::PayForTxFailed(err.glue_into()))
            }
            crate::vm_1_3_2::TxRevertReason::FailedToMarkFactoryDependencies(err) => Self::Halt(
                crate::interface::Halt::FailedToMarkFactoryDependencies(err.glue_into()),
            ),
            crate::vm_1_3_2::TxRevertReason::FailedToChargeFee(err) => {
                Self::Halt(crate::interface::Halt::FailedToChargeFee(err.glue_into()))
            }
            crate::vm_1_3_2::TxRevertReason::FromIsNotAnAccount => {
                Self::Halt(crate::interface::Halt::FromIsNotAnAccount)
            }
            crate::vm_1_3_2::TxRevertReason::InnerTxError => {
                Self::Halt(crate::interface::Halt::InnerTxError)
            }
            crate::vm_1_3_2::TxRevertReason::Unknown(err) => {
                Self::Halt(crate::interface::Halt::Unknown(err.glue_into()))
            }
            crate::vm_1_3_2::TxRevertReason::UnexpectedVMBehavior(err) => {
                Self::Halt(crate::interface::Halt::UnexpectedVMBehavior(err))
            }
            crate::vm_1_3_2::TxRevertReason::BootloaderOutOfGas => {
                Self::Halt(crate::interface::Halt::BootloaderOutOfGas)
            }
            crate::vm_1_3_2::TxRevertReason::TooBigGasLimit => {
                Self::Halt(crate::interface::Halt::TooBigGasLimit)
            }
            crate::vm_1_3_2::TxRevertReason::NotEnoughGasProvided => {
                Self::Halt(crate::interface::Halt::NotEnoughGasProvided)
            }
            crate::vm_1_3_2::TxRevertReason::MissingInvocationLimitReached => {
                Self::Halt(crate::interface::Halt::MissingInvocationLimitReached)
            }
        }
    }
}
