#![deny(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_imports)]

pub use zk_evm_1_4_1 as zk_evm_latest;
pub use zkevm_test_harness_1_4_1 as zkevm_test_harness_latest;
pub use zksync_types::vm_version::VmVersion;

pub use self::versions::{
    vm_1_3_2, vm_boojum_integration, vm_latest, vm_m5, vm_m6, vm_refunds_enhancement,
    vm_virtual_blocks,
};
pub use crate::{
    glue::{
        history_mode::HistoryMode,
        tracers::{MultiVMTracer, MultiVmTracerPointer},
    },
    vm_instance::VmInstance,
};

mod glue;
pub mod interface;
pub mod tracers;
pub mod utils;
pub mod versions;
mod vm_instance;
