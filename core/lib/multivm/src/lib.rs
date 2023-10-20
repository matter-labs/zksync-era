// #![deny(unreachable_pub)]
#![deny(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_imports)]

pub use crate::{
    glue::{
        block_properties::BlockProperties, history_mode::HistoryMode, oracle_tools::OracleTools,
    },
    vm_instance::VmInstance,
};
pub use zksync_types::vm_version::VmVersion;

mod glue;
mod vm_instance;

pub mod interface;
mod tracers;
pub mod versions;

pub use versions::vm_1_3_2;
pub use versions::vm_latest;
pub use versions::vm_m5;
pub use versions::vm_m6;
pub use versions::vm_virtual_blocks;
