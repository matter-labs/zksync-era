use crate::glue::GlueFrom;

impl GlueFrom<vm_latest::TxExecutionMode> for vm_m5::vm_with_bootloader::TxExecutionMode {
    fn glue_from(value: vm_latest::TxExecutionMode) -> Self {
        match value {
            vm_latest::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            vm_latest::TxExecutionMode::EstimateFee => Self::EstimateFee,
            vm_latest::TxExecutionMode::EthCall => Self::EthCall,
        }
    }
}

impl GlueFrom<vm_latest::TxExecutionMode> for vm_m6::vm_with_bootloader::TxExecutionMode {
    fn glue_from(value: vm_latest::TxExecutionMode) -> Self {
        match value {
            vm_latest::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            vm_latest::TxExecutionMode::EstimateFee => Self::EstimateFee {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
            vm_latest::TxExecutionMode::EthCall => Self::EthCall {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
        }
    }
}

impl GlueFrom<vm_latest::TxExecutionMode> for vm_1_3_2::vm_with_bootloader::TxExecutionMode {
    fn glue_from(value: vm_latest::TxExecutionMode) -> Self {
        match value {
            vm_latest::TxExecutionMode::VerifyExecute => Self::VerifyExecute,
            vm_latest::TxExecutionMode::EstimateFee => Self::EstimateFee {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
            vm_latest::TxExecutionMode::EthCall => Self::EthCall {
                // We used it only for api services we don't have limit for storage invocation inside statekeeper
                // It's impossible to recover this value for the vm integration after virtual blocks
                missed_storage_invocation_limit: usize::MAX,
            },
        }
    }
}
