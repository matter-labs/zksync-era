use crate::glue::{GlueFrom, GlueInto};
use crate::vm_latest::Halt;

impl GlueFrom<crate::vm_virtual_blocks::Halt> for Halt {
    fn glue_from(value: crate::vm_virtual_blocks::Halt) -> Self {
        match value {
            crate::vm_virtual_blocks::Halt::ValidationFailed(reason) => {
                Self::ValidationFailed(reason.glue_into())
            }
            crate::vm_virtual_blocks::Halt::PaymasterValidationFailed(reason) => {
                Self::PaymasterValidationFailed(reason.glue_into())
            }
            crate::vm_virtual_blocks::Halt::PrePaymasterPreparationFailed(reason) => {
                Self::PrePaymasterPreparationFailed(reason.glue_into())
            }
            crate::vm_virtual_blocks::Halt::PayForTxFailed(reason) => {
                Self::PayForTxFailed(reason.glue_into())
            }
            crate::vm_virtual_blocks::Halt::FailedToMarkFactoryDependencies(reason) => {
                Self::FailedToMarkFactoryDependencies(reason.glue_into())
            }
            crate::vm_virtual_blocks::Halt::FailedToChargeFee(reason) => {
                Self::FailedToChargeFee(reason.glue_into())
            }
            crate::vm_virtual_blocks::Halt::FromIsNotAnAccount => Self::FromIsNotAnAccount,
            crate::vm_virtual_blocks::Halt::InnerTxError => Self::InnerTxError,
            crate::vm_virtual_blocks::Halt::Unknown(reason) => Self::Unknown(reason.glue_into()),
            crate::vm_virtual_blocks::Halt::UnexpectedVMBehavior(reason) => {
                Self::UnexpectedVMBehavior(reason)
            }
            crate::vm_virtual_blocks::Halt::BootloaderOutOfGas => Self::BootloaderOutOfGas,
            crate::vm_virtual_blocks::Halt::TooBigGasLimit => Self::TooBigGasLimit,
            crate::vm_virtual_blocks::Halt::NotEnoughGasProvided => Self::NotEnoughGasProvided,
            crate::vm_virtual_blocks::Halt::MissingInvocationLimitReached => {
                Self::MissingInvocationLimitReached
            }
            crate::vm_virtual_blocks::Halt::FailedToSetL2Block(reason) => {
                Self::FailedToSetL2Block(reason)
            }
            crate::vm_virtual_blocks::Halt::FailedToAppendTransactionToL2Block(reason) => {
                Self::FailedToAppendTransactionToL2Block(reason)
            }
            crate::vm_virtual_blocks::Halt::VMPanic => Self::VMPanic,
        }
    }
}
