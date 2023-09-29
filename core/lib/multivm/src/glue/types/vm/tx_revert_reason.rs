use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::TxRevertReason> for vm_virtual_blocks::TxRevertReason {
    fn glue_from(value: vm_m5::TxRevertReason) -> Self {
        match value {
            vm_m5::TxRevertReason::EthCall(err) => Self::TxReverted(err.glue_into()),
            vm_m5::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            vm_m5::TxRevertReason::ValidationFailed(err) => {
                Self::Halt(vm_virtual_blocks::Halt::ValidationFailed(err.glue_into()))
            }
            vm_m5::TxRevertReason::PaymasterValidationFailed(err) => Self::Halt(
                vm_virtual_blocks::Halt::PaymasterValidationFailed(err.glue_into()),
            ),
            vm_m5::TxRevertReason::PrePaymasterPreparationFailed(err) => Self::Halt(
                vm_virtual_blocks::Halt::PrePaymasterPreparationFailed(err.glue_into()),
            ),
            vm_m5::TxRevertReason::PayForTxFailed(err) => {
                Self::Halt(vm_virtual_blocks::Halt::PayForTxFailed(err.glue_into()))
            }
            vm_m5::TxRevertReason::FailedToMarkFactoryDependencies(err) => Self::Halt(
                vm_virtual_blocks::Halt::FailedToMarkFactoryDependencies(err.glue_into()),
            ),
            vm_m5::TxRevertReason::FailedToChargeFee(err) => {
                Self::Halt(vm_virtual_blocks::Halt::FailedToChargeFee(err.glue_into()))
            }
            vm_m5::TxRevertReason::FromIsNotAnAccount => {
                Self::Halt(vm_virtual_blocks::Halt::FromIsNotAnAccount)
            }
            vm_m5::TxRevertReason::InnerTxError => {
                Self::Halt(vm_virtual_blocks::Halt::InnerTxError)
            }
            vm_m5::TxRevertReason::Unknown(err) => {
                Self::Halt(vm_virtual_blocks::Halt::Unknown(err.glue_into()))
            }
            vm_m5::TxRevertReason::UnexpectedVMBehavior(err) => {
                Self::Halt(vm_virtual_blocks::Halt::UnexpectedVMBehavior(err))
            }
            vm_m5::TxRevertReason::BootloaderOutOfGas => {
                Self::Halt(vm_virtual_blocks::Halt::BootloaderOutOfGas)
            }
            vm_m5::TxRevertReason::TooBigGasLimit => {
                Self::Halt(vm_virtual_blocks::Halt::TooBigGasLimit)
            }
            vm_m5::TxRevertReason::NotEnoughGasProvided => {
                Self::Halt(vm_virtual_blocks::Halt::NotEnoughGasProvided)
            }
        }
    }
}

impl GlueFrom<vm_m6::TxRevertReason> for vm_virtual_blocks::TxRevertReason {
    fn glue_from(value: vm_m6::TxRevertReason) -> Self {
        match value {
            vm_m6::TxRevertReason::EthCall(err) => Self::TxReverted(err.glue_into()),
            vm_m6::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            vm_m6::TxRevertReason::ValidationFailed(err) => {
                Self::Halt(vm_virtual_blocks::Halt::ValidationFailed(err.glue_into()))
            }
            vm_m6::TxRevertReason::PaymasterValidationFailed(err) => Self::Halt(
                vm_virtual_blocks::Halt::PaymasterValidationFailed(err.glue_into()),
            ),
            vm_m6::TxRevertReason::PrePaymasterPreparationFailed(err) => Self::Halt(
                vm_virtual_blocks::Halt::PrePaymasterPreparationFailed(err.glue_into()),
            ),
            vm_m6::TxRevertReason::PayForTxFailed(err) => {
                Self::Halt(vm_virtual_blocks::Halt::PayForTxFailed(err.glue_into()))
            }
            vm_m6::TxRevertReason::FailedToMarkFactoryDependencies(err) => Self::Halt(
                vm_virtual_blocks::Halt::FailedToMarkFactoryDependencies(err.glue_into()),
            ),
            vm_m6::TxRevertReason::FailedToChargeFee(err) => {
                Self::Halt(vm_virtual_blocks::Halt::FailedToChargeFee(err.glue_into()))
            }
            vm_m6::TxRevertReason::FromIsNotAnAccount => {
                Self::Halt(vm_virtual_blocks::Halt::FromIsNotAnAccount)
            }
            vm_m6::TxRevertReason::InnerTxError => {
                Self::Halt(vm_virtual_blocks::Halt::InnerTxError)
            }
            vm_m6::TxRevertReason::Unknown(err) => {
                Self::Halt(vm_virtual_blocks::Halt::Unknown(err.glue_into()))
            }
            vm_m6::TxRevertReason::UnexpectedVMBehavior(err) => {
                Self::Halt(vm_virtual_blocks::Halt::UnexpectedVMBehavior(err))
            }
            vm_m6::TxRevertReason::BootloaderOutOfGas => {
                Self::Halt(vm_virtual_blocks::Halt::BootloaderOutOfGas)
            }
            vm_m6::TxRevertReason::TooBigGasLimit => {
                Self::Halt(vm_virtual_blocks::Halt::TooBigGasLimit)
            }
            vm_m6::TxRevertReason::NotEnoughGasProvided => {
                Self::Halt(vm_virtual_blocks::Halt::NotEnoughGasProvided)
            }
            vm_m6::TxRevertReason::MissingInvocationLimitReached => {
                Self::Halt(vm_virtual_blocks::Halt::MissingInvocationLimitReached)
            }
        }
    }
}

impl GlueFrom<vm_1_3_2::TxRevertReason> for vm_virtual_blocks::TxRevertReason {
    fn glue_from(value: vm_1_3_2::TxRevertReason) -> Self {
        match value {
            vm_1_3_2::TxRevertReason::EthCall(err) => Self::TxReverted(err.glue_into()),
            vm_1_3_2::TxRevertReason::TxReverted(err) => Self::TxReverted(err.glue_into()),
            vm_1_3_2::TxRevertReason::ValidationFailed(err) => {
                Self::Halt(vm_virtual_blocks::Halt::ValidationFailed(err.glue_into()))
            }
            vm_1_3_2::TxRevertReason::PaymasterValidationFailed(err) => Self::Halt(
                vm_virtual_blocks::Halt::PaymasterValidationFailed(err.glue_into()),
            ),
            vm_1_3_2::TxRevertReason::PrePaymasterPreparationFailed(err) => Self::Halt(
                vm_virtual_blocks::Halt::PrePaymasterPreparationFailed(err.glue_into()),
            ),
            vm_1_3_2::TxRevertReason::PayForTxFailed(err) => {
                Self::Halt(vm_virtual_blocks::Halt::PayForTxFailed(err.glue_into()))
            }
            vm_1_3_2::TxRevertReason::FailedToMarkFactoryDependencies(err) => Self::Halt(
                vm_virtual_blocks::Halt::FailedToMarkFactoryDependencies(err.glue_into()),
            ),
            vm_1_3_2::TxRevertReason::FailedToChargeFee(err) => {
                Self::Halt(vm_virtual_blocks::Halt::FailedToChargeFee(err.glue_into()))
            }
            vm_1_3_2::TxRevertReason::FromIsNotAnAccount => {
                Self::Halt(vm_virtual_blocks::Halt::FromIsNotAnAccount)
            }
            vm_1_3_2::TxRevertReason::InnerTxError => {
                Self::Halt(vm_virtual_blocks::Halt::InnerTxError)
            }
            vm_1_3_2::TxRevertReason::Unknown(err) => {
                Self::Halt(vm_virtual_blocks::Halt::Unknown(err.glue_into()))
            }
            vm_1_3_2::TxRevertReason::UnexpectedVMBehavior(err) => {
                Self::Halt(vm_virtual_blocks::Halt::UnexpectedVMBehavior(err))
            }
            vm_1_3_2::TxRevertReason::BootloaderOutOfGas => {
                Self::Halt(vm_virtual_blocks::Halt::BootloaderOutOfGas)
            }
            vm_1_3_2::TxRevertReason::TooBigGasLimit => {
                Self::Halt(vm_virtual_blocks::Halt::TooBigGasLimit)
            }
            vm_1_3_2::TxRevertReason::NotEnoughGasProvided => {
                Self::Halt(vm_virtual_blocks::Halt::NotEnoughGasProvided)
            }
            vm_1_3_2::TxRevertReason::MissingInvocationLimitReached => {
                Self::Halt(vm_virtual_blocks::Halt::MissingInvocationLimitReached)
            }
        }
    }
}
