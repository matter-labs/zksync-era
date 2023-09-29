use crate::glue::GlueFrom;

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::TxExecutionMode>
    for vm_m5::vm_with_bootloader::TxExecutionMode
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::TxExecutionMode) -> Self {
        match value {
            vm_vm1_3_2::vm_with_bootloader::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            vm_vm1_3_2::vm_with_bootloader::TxExecutionMode::EstimateFee { .. } => {
                Self::EstimateFee
            }
            vm_vm1_3_2::vm_with_bootloader::TxExecutionMode::EthCall { .. } => Self::EthCall,
        }
    }
}

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::TxExecutionMode>
    for vm_m6::vm_with_bootloader::TxExecutionMode
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::TxExecutionMode) -> Self {
        match value {
            vm_vm1_3_2::vm_with_bootloader::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            vm_vm1_3_2::vm_with_bootloader::TxExecutionMode::EstimateFee {
                missed_storage_invocation_limit,
            } => Self::EstimateFee {
                missed_storage_invocation_limit,
            },
            vm_vm1_3_2::vm_with_bootloader::TxExecutionMode::EthCall {
                missed_storage_invocation_limit,
            } => Self::EthCall {
                missed_storage_invocation_limit,
            },
        }
    }
}
