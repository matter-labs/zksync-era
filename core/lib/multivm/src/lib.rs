#![deny(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_imports)]

pub use circuit_sequencer_api as circuit_sequencer_api_latest;
pub use zk_evm_1_5_2 as zk_evm_latest;
pub use zksync_types::vm::VmVersion;
pub use zksync_vm_interface as interface;

pub use crate::{
    glue::{
        history_mode::HistoryMode,
        tracers::{IntoOldVmTracer, MultiVmTracer, MultiVmTracerPointer},
    },
    versions::{
        vm_1_3_2, vm_1_4_1, vm_1_4_2, vm_boojum_integration, vm_fast, vm_latest, vm_m5, vm_m6,
        vm_refunds_enhancement, vm_virtual_blocks,
    },
    vm_instance::{is_supported_by_fast_vm, FastVmInstance, LegacyVmInstance},
};

mod glue;
pub mod pubdata_builders;
pub mod tracers;
pub mod utils;
mod versions;
mod vm_instance;
