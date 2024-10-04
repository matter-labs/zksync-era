use zk_os_forward_system::run::run_batch;
use zksync_types::Transaction;
use zksync_vm_interface::storage::WriteStorage;
use zksync_vm_interface::{BytecodeCompressionResult, FinishedL1Batch, L2BlockEnv, VmExecutionMode, VmExecutionResultAndLogs, VmInterface, VmInterfaceExt, VmInterfaceHistoryEnabled, VmMemoryMetrics};
use crate::HistoryMode;
use crate::vm_latest::Vm;

#[derive(Debug)]
pub struct VmZkOs {

}

impl VmInterface for VmZkOs {
    type TracerDispatcher = ();

    fn push_transaction(&mut self, tx: Transaction) {

    }

    fn inspect(&mut self, dispatcher: Self::TracerDispatcher, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        // run_batch(
        //
        // )
        todo!()
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        todo!()
    }

    fn inspect_transaction_with_bytecode_compression(&mut self, tracer: Self::TracerDispatcher, tx: Transaction, with_compression: bool) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        todo!()
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        todo!()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        todo!()
    }
}

impl VmInterfaceHistoryEnabled for VmZkOs {
    fn make_snapshot(&mut self) {
        todo!()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        todo!()
    }

    fn pop_snapshot_no_rollback(&mut self) {
        todo!()
    }
}