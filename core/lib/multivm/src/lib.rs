pub use crate::{
    glue::{
        block_properties::BlockProperties,
        init_vm::{init_vm, init_vm_with_gas_limit},
        oracle_tools::OracleTools,
    },
    vm_instance::VmInstance,
};

mod glue;
mod vm_instance;

/// Marker of the VM version to be used by the MultiVM.
#[derive(Debug, Clone, Copy)]
pub enum VmVersion {
    M5WithoutRefunds,
    M5WithRefunds,
    M6Initial,
    M6BugWithCompressionFixed,
    Vm1_3_2,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        Self::Vm1_3_2
    }
}
