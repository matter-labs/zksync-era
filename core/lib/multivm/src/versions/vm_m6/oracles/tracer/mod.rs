use zk_evm_1_3_1::abstractions::Tracer;
use zk_evm_1_3_1::vm_state::VmLocalState;

mod bootloader;
mod call;
mod one_tx;
mod transaction_result;
mod utils;
mod validation;

pub use bootloader::BootloaderTracer;
pub use call::CallTracer;
pub use one_tx::OneTxTracer;
pub use validation::{
    ValidationError, ValidationTracer, ValidationTracerParams, ViolatedValidationRule,
};

pub(crate) use transaction_result::TransactionResultTracer;

use crate::vm_m6::history_recorder::HistoryMode;
use crate::vm_m6::memory::SimpleMemory;

pub trait ExecutionEndTracer<H: HistoryMode>: Tracer<SupportedMemory = SimpleMemory<H>> {
    // Returns whether the vm execution should stop.
    fn should_stop_execution(&self) -> bool;
}

pub trait PendingRefundTracer<H: HistoryMode>: Tracer<SupportedMemory = SimpleMemory<H>> {
    /// Some(x) means that the bootloader has asked the operator to provide the refund for the
    /// transaction, where `x` is the refund that the bootloader has suggested on its own.
    fn requested_refund(&self) -> Option<u32> {
        None
    }

    /// Set the current request for refund as fulfilled
    fn set_refund_as_done(&mut self) {}
}

pub trait PubdataSpentTracer<H: HistoryMode>: Tracer<SupportedMemory = SimpleMemory<H>> {
    /// Returns how much gas was spent on pubdata.
    fn gas_spent_on_pubdata(&self, _vm_local_state: &VmLocalState) -> u32 {
        0
    }
}

pub trait StorageInvocationTracer<H: HistoryMode>:
    Tracer<SupportedMemory = SimpleMemory<H>>
{
    /// Set how many invocation of the storage oracle were missed.
    fn set_missed_storage_invocations(&mut self, _missed_storage_invocation: usize) {}
    fn is_limit_reached(&self) -> bool {
        false
    }
}
