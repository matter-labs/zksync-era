//! This module contains traits for the VM interface.
//! All VMs should implement these traits to be used in our system.
//! The trait is generic over the storage type, allowing it to be used with any storage implementation.
//! Additionally, this trait is generic over HistoryMode, allowing it to be used with or without history.
//!
//! `TracerDispatcher` is an associated type used to dispatch tracers in VMs.
//! It manages tracers across different VM versions.
//! Even though we use the same interface for all VM versions,
//! we can now specify only the necessary trait bounds for each VM version.
//!
//! Generally speaking, in most cases, the tracer dispatcher is a wrapper around `Vec<Box<dyn VmTracer>>`,
//! where `VmTracer` is a trait implemented for a specific VM version.

use zksync_types::Transaction;

use crate::{
    storage::StoragePtr, BootloaderMemory, BytecodeCompressionError, CompressedBytecodeInfo,
    CurrentExecutionState, FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode,
    VmExecutionResultAndLogs, VmMemoryMetrics,
};

pub trait VmInterface {
    type TracerDispatcher: Default;

    /// Push transaction to bootloader memory.
    fn push_transaction(&mut self, tx: Transaction);

    /// Execute next VM step (either next transaction or bootloader or the whole batch).
    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        self.inspect(Self::TracerDispatcher::default(), execution_mode)
    }

    /// Execute next VM step (either next transaction or bootloader or the whole batch)
    /// with custom tracers.
    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs;

    /// Get bootloader memory.
    fn get_bootloader_memory(&self) -> BootloaderMemory;

    /// Get last transaction's compressed bytecodes.
    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo>;

    /// Start a new L2 block.
    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv);

    /// Get the current state of the virtual machine.
    /// This method should be used only after the batch execution; otherwise, it can panic.
    fn get_current_execution_state(&self) -> CurrentExecutionState;

    /// Execute transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        self.inspect_transaction_with_bytecode_compression(
            Self::TracerDispatcher::default(),
            tx,
            with_compression,
        )
    }

    /// Execute transaction with optional bytecode compression using custom tracers.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    );

    /// Record VM memory metrics.
    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics;

    /// How much gas is left in the current stack frame.
    fn gas_remaining(&self) -> u32;

    /// Execute batch till the end and return the result, with final execution state
    /// and bootloader memory.
    fn finish_batch(&mut self) -> FinishedL1Batch {
        let result = self.execute(VmExecutionMode::Batch);
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self.get_bootloader_memory();
        FinishedL1Batch {
            block_tip_execution_result: result,
            final_execution_state: execution_state,
            final_bootloader_memory: Some(bootloader_memory),
            pubdata_input: None,
            state_diffs: None,
        }
    }
}

/// Encapsulates creating VM instance based on the provided environment.
pub trait VmFactory<S>: VmInterface {
    /// Creates a new VM instance.
    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self;
}

/// Methods of VM requiring history manipulations.
///
/// # Snapshot workflow
///
/// External callers must follow the following snapshot workflow:
///
/// - Each new snapshot created using `make_snapshot()` must be either popped or rolled back before creating the following snapshot.
///   OTOH, it's not required to call either of these methods by the end of VM execution.
/// - `pop_snapshot_no_rollback()` may be called spuriously, when no snapshot was created. It is a no-op in this case.
///
/// These rules guarantee that at each given moment, a VM instance has at most one snapshot (unless the VM makes snapshots internally),
/// which may allow additional VM optimizations.
pub trait VmInterfaceHistoryEnabled: VmInterface {
    /// Create a snapshot of the current VM state and push it into memory.
    fn make_snapshot(&mut self);

    /// Roll back VM state to the latest snapshot and destroy the snapshot.
    fn rollback_to_the_latest_snapshot(&mut self);

    /// Pop the latest snapshot from memory and destroy it. If there are no snapshots, this should be a no-op
    /// (i.e., the VM must not panic in this case).
    fn pop_snapshot_no_rollback(&mut self);
}
