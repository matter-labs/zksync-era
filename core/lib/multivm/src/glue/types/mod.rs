//! Glue for the basic types that are used in the VM.
//! This is "internal" glue that generally converts the "latest" input type to the target
//! "VM" type (e.g. "latest" -> "vm_m5"), and then converts the "VM" output type to the
//! "latest" output type (e.g. "vm_m5" -> "latest").
//!
//! This "glue layer" is generally not visible outside of the crate.

mod vm;
mod zk_evm_1_3_1;
mod zk_evm_1_3_3;
mod zk_evm_1_4_0;
mod zk_evm_1_4_1;
mod zk_evm_1_5_0;
mod zk_evm_1_5_2;
