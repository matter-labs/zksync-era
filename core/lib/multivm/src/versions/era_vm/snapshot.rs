use super::bootloader_state::BootloaderStateSnapshot;

pub struct VmSnapshot {
    // execution: era_vm::execution::Execution,
    pub vm_snapshot: era_vm::vm::VmSnapshot,
    pub(crate) bootloader_snapshot: BootloaderStateSnapshot,
    pub suspended_at: u16,
    pub(crate) gas_for_account_validation: u32,
}
