//! Glue for the basic types that are used in the VM.
//! This is "internal" glue that generally converts the "latest" input type to the target
//! "VM" type (e.g. "latest" -> "vm_m5"), and then converts the "VM" output type to the
//! "latest" output type (e.g. "vm_m5" -> "latest").
//!
//! This "glue layer" is generally not visible outside of the crate.

use zksync_types::{l2_to_l1_log::L2ToL1Log, zk_evm_types::EventMessage};

use super::GlueInto;

mod vm;
mod zk_evm_1_3_1;
mod zk_evm_1_3_3;
mod zk_evm_1_4_0;
mod zk_evm_1_4_1;

// pub(crate) trait IntoL2ToL1Log {
//     fn into_l2_to_l1_log(self) -> L2ToL1Log;
// }

// impl<T: GlueInto<EventMessage>> IntoL2ToL1Log for T {
//     fn into_l2_to_l1_log(self) -> L2ToL1Log {
//         L2ToL1Log::from(self.glue_into())
//     }
// }
