use zk_evm_1_3_3::vm_state::VmLocalState;

use crate::vm_virtual_blocks::bootloader_state::BootloaderStateSnapshot;

/// A snapshot of the VM that holds enough information to
/// rollback the VM to some historical state.
#[derive(Debug, Clone)]
pub(crate) struct VmSnapshot {
    pub(crate) local_state: VmLocalState,
    pub(crate) bootloader_state: BootloaderStateSnapshot,
}
