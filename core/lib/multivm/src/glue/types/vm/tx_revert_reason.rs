use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::TxRevertReason> for vm_vm1_3_2::TxRevertReason {
    fn glue_from(value: vm_m5::TxRevertReason) -> Self {
        match value {
            vm_m5::TxRevertReason::EthCall(err) => Self::EthCall(err.glue_into()),
            vm_m5::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            vm_m5::TxRevertReason::ValidationFailed(err) => Self::ValidationFailed(err.glue_into()),
            vm_m5::TxRevertReason::PaymasterValidationFailed(err) => {
                Self::PaymasterValidationFailed(err.glue_into())
            }
            vm_m5::TxRevertReason::PrePaymasterPreparationFailed(err) => {
                Self::PrePaymasterPreparationFailed(err.glue_into())
            }
            vm_m5::TxRevertReason::PayForTxFailed(err) => Self::PayForTxFailed(err.glue_into()),
            vm_m5::TxRevertReason::FailedToMarkFactoryDependencies(err) => {
                Self::FailedToMarkFactoryDependencies(err.glue_into())
            }
            vm_m5::TxRevertReason::FailedToChargeFee(err) => {
                Self::FailedToChargeFee(err.glue_into())
            }
            vm_m5::TxRevertReason::FromIsNotAnAccount => Self::FromIsNotAnAccount,
            vm_m5::TxRevertReason::InnerTxError => Self::InnerTxError,
            vm_m5::TxRevertReason::Unknown(err) => Self::Unknown(err.glue_into()),
            vm_m5::TxRevertReason::UnexpectedVMBehavior(err) => Self::UnexpectedVMBehavior(err),
            vm_m5::TxRevertReason::BootloaderOutOfGas => Self::BootloaderOutOfGas,
            vm_m5::TxRevertReason::TooBigGasLimit => Self::TooBigGasLimit,
            vm_m5::TxRevertReason::NotEnoughGasProvided => Self::NotEnoughGasProvided,
        }
    }
}

impl GlueFrom<vm_m6::TxRevertReason> for vm_vm1_3_2::TxRevertReason {
    fn glue_from(value: vm_m6::TxRevertReason) -> Self {
        match value {
            vm_m6::TxRevertReason::EthCall(err) => Self::EthCall(err.glue_into()),
            vm_m6::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            vm_m6::TxRevertReason::ValidationFailed(err) => Self::ValidationFailed(err.glue_into()),
            vm_m6::TxRevertReason::PaymasterValidationFailed(err) => {
                Self::PaymasterValidationFailed(err.glue_into())
            }
            vm_m6::TxRevertReason::PrePaymasterPreparationFailed(err) => {
                Self::PrePaymasterPreparationFailed(err.glue_into())
            }
            vm_m6::TxRevertReason::PayForTxFailed(err) => Self::PayForTxFailed(err.glue_into()),
            vm_m6::TxRevertReason::FailedToMarkFactoryDependencies(err) => {
                Self::FailedToMarkFactoryDependencies(err.glue_into())
            }
            vm_m6::TxRevertReason::FailedToChargeFee(err) => {
                Self::FailedToChargeFee(err.glue_into())
            }
            vm_m6::TxRevertReason::FromIsNotAnAccount => Self::FromIsNotAnAccount,
            vm_m6::TxRevertReason::InnerTxError => Self::InnerTxError,
            vm_m6::TxRevertReason::Unknown(err) => Self::Unknown(err.glue_into()),
            vm_m6::TxRevertReason::UnexpectedVMBehavior(err) => Self::UnexpectedVMBehavior(err),
            vm_m6::TxRevertReason::BootloaderOutOfGas => Self::BootloaderOutOfGas,
            vm_m6::TxRevertReason::TooBigGasLimit => Self::TooBigGasLimit,
            vm_m6::TxRevertReason::NotEnoughGasProvided => Self::NotEnoughGasProvided,
            vm_m6::TxRevertReason::MissingInvocationLimitReached => {
                Self::MissingInvocationLimitReached
            }
        }
    }
}
