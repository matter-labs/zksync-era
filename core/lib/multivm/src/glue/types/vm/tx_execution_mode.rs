use crate::glue::GlueFrom;

impl GlueFrom<crate::interface::TxExecutionMode>
    for crate::vm_m5::vm_with_bootloader::TxExecutionMode
{
    fn glue_from(value: crate::interface::TxExecutionMode) -> Self {
        match value {
            crate::interface::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            crate::interface::TxExecutionMode::EstimateFee => Self::EstimateFee,
            crate::interface::TxExecutionMode::EthCall => Self::EthCall,
        }
    }
}

impl GlueFrom<crate::interface::TxExecutionMode>
    for crate::vm_m6::vm_with_bootloader::TxExecutionMode
{
    fn glue_from(value: crate::interface::TxExecutionMode) -> Self {
        match value {
            crate::interface::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            crate::interface::TxExecutionMode::EstimateFee => Self::EstimateFee {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
            crate::interface::TxExecutionMode::EthCall => Self::EthCall {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
        }
    }
}

impl GlueFrom<crate::interface::TxExecutionMode>
    for crate::vm_1_3_2::vm_with_bootloader::TxExecutionMode
{
    fn glue_from(value: crate::interface::TxExecutionMode) -> Self {
        match value {
            crate::interface::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            crate::interface::TxExecutionMode::EstimateFee => Self::EstimateFee {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
            crate::interface::TxExecutionMode::EthCall => Self::EthCall {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
        }
    }
}
