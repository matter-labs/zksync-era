use crate::memory::SimpleMemory;
use zk_evm::abstractions::Tracer;
use zk_evm::vm_state::VmLocalState;

mod bootloader;
mod one_tx;
mod transaction_result;
mod utils;
mod validation;

pub use bootloader::BootloaderTracer;
pub use one_tx::OneTxTracer;
pub use validation::{ValidationError, ValidationTracer, ValidationTracerParams};

pub(crate) use transaction_result::TransactionResultTracer;

pub trait ExecutionEndTracer: Tracer<SupportedMemory = SimpleMemory> {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> bool;
}

pub trait PendingRefundTracer: Tracer<SupportedMemory = SimpleMemory> {
    // Some(x) means that the bootloader has asked the operator to provide the refund for the
    // transaction, where `x` is the refund that the bootloader has suggested on its own.
    fn requested_refund(&self) -> Option<u32> {
        None
    }

    // Set the current request for refund as fulfilled
    fn set_refund_as_done(&mut self) {}
}

pub trait PubdataSpentTracer: Tracer<SupportedMemory = SimpleMemory> {
    // Returns how much gas was spent on pubdata.
    fn gas_spent_on_pubdata(&self, _vm_local_state: &VmLocalState) -> u32 {
        0
    }
}
