pub use self::{
    bootloader_error::BootloaderErrorCode,
    bytecode_compression::BytecodeCompressionError,
    halt::Halt,
    tx_revert_reason::TxRevertReason,
    vm_revert_reason::{VmRevertReason, VmRevertReasonParsingError},
};

mod bootloader_error;
mod bytecode_compression;
mod halt;
mod tx_revert_reason;
mod vm_revert_reason;

// FIXME: make errors non-exhaustive?
