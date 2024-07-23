#![deny(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_imports)]

pub use circuit_sequencer_api_1_5_0 as circuit_sequencer_api_latest;
pub use zk_evm_1_5_0 as zk_evm_latest;
pub use zksync_types::vm::VmVersion;

pub use crate::{
    glue::{
        history_mode::HistoryMode,
        tracers::{MultiVMTracer, MultiVmTracerPointer},
    },
    versions::{
        vm_1_3_2, vm_1_4_1, vm_1_4_2, vm_boojum_integration, vm_fast, vm_latest, vm_m5, vm_m6,
        vm_refunds_enhancement, vm_virtual_blocks, era_vm,
    },
    vm_instance::VmInstance,
};

mod glue;
pub mod interface;
pub mod tracers;
pub mod utils;
pub mod versions;
mod vm_instance;
