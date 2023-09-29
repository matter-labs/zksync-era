use crate::glue::GlueFrom;

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::BootloaderJobType>
    for vm_m5::vm_with_bootloader::BootloaderJobType
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::BootloaderJobType) -> Self {
        match value {
            vm_vm1_3_2::vm_with_bootloader::BootloaderJobType::TransactionExecution => {
                Self::TransactionExecution
            }
            vm_vm1_3_2::vm_with_bootloader::BootloaderJobType::BlockPostprocessing => {
                Self::BlockPostprocessing
            }
        }
    }
}

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::BootloaderJobType>
    for vm_m6::vm_with_bootloader::BootloaderJobType
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::BootloaderJobType) -> Self {
        match value {
            vm_vm1_3_2::vm_with_bootloader::BootloaderJobType::TransactionExecution => {
                Self::TransactionExecution
            }
            vm_vm1_3_2::vm_with_bootloader::BootloaderJobType::BlockPostprocessing => {
                Self::BlockPostprocessing
            }
        }
    }
}
