pub(crate) use pubdata::PubdataInput;
pub(crate) use snapshot::VmSnapshot;
pub(crate) use transaction_data::TransactionData;
pub(crate) use vm_state::new_vm_state;
pub use vm_state::ZkSyncVmState;
pub(crate) mod pubdata;
mod snapshot;
mod transaction_data;
mod vm_state;
