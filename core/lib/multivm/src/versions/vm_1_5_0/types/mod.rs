mod hook;
mod snapshot;
mod transaction_data;
mod vm_state;

pub use self::vm_state::ZkSyncVmState;
pub(crate) use self::{
    hook::VmHook, snapshot::VmSnapshot, transaction_data::TransactionData, vm_state::new_vm_state,
};
