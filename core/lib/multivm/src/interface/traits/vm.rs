use crate::interface::traits::tracers::multivm_tracer::MultivmTracer;
use crate::interface::types::errors::BytecodeCompressionError;
use crate::interface::types::inputs::{L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode};
use crate::interface::types::outputs::{
    BootloaderMemory, CurrentExecutionState, VmExecutionResultAndLogs,
};
use crate::vm_latest::HistoryEnabled;
use crate::HistoryMode;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::Transaction;
use zksync_utils::bytecode::CompressedBytecodeInfo;

/// Public interface for VM
pub trait VmInterface<S: WriteStorage, H: HistoryMode> {
    fn push_transaction(&mut self, tx: Transaction);
    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        todo!()
    }
    fn inspect<T: MultivmTracer<S, H>>(
        &mut self,
        tracer: T,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs;
    fn get_bootloader_memory(&self) -> BootloaderMemory;
    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo>;
    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv);
    /// Get current state of virtual machine.
    /// This method should be used only after the batch execution.
    /// Otherwise it can panic.
    fn get_current_execution_state(&self) -> CurrentExecutionState;

    /// Execute transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, BytecodeCompressionError> {
        todo!()
    }

    /// Inspect transaction with optional bytecode compression.
    fn inspect_transaction_with_bytecode_compression<T: MultivmTracer<S, H>>(
        &mut self,
        tracer: T,
        tx: Transaction,
        with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, BytecodeCompressionError>;
}

/// Methods of vm, which required some history manipullations
pub trait VmInterfaceHistoryEnabled<S: WriteStorage>: VmInterface<S, HistoryEnabled> {
    /// Create snapshot of current vm state and push it into the memory
    fn make_snapshot(&mut self);

    /// Rollback vm state to the latest snapshot and destroy the snapshot
    fn rollback_to_the_latest_snapshot(&mut self);

    /// Pop the latest snapshot from the memory and destroy it
    fn pop_snapshot_no_rollback(&mut self);
}
