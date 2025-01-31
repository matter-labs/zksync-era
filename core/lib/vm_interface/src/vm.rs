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

use std::rc::Rc;

use zksync_types::{Transaction, H256};

use crate::{
    pubdata::PubdataBuilder, storage::StoragePtr, BytecodeCompressionResult, FinishedL1Batch,
    InspectExecutionMode, L1BatchEnv, L2BlockEnv, PushTransactionResult, SystemEnv,
    VmExecutionResultAndLogs,
};

pub trait VmInterface {
    type TracerDispatcher: Default;

    /// Pushes a transaction to bootloader memory for future execution with bytecode compression (if it's supported by the VM).
    ///
    /// # Return value
    ///
    /// Returns preprocessing results, such as compressed bytecodes. The results may borrow from the VM state,
    /// so you may want to inspect results before next operations with the VM, or clone the necessary parts.
    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_>;

    /// Executes the next VM step (either next transaction or bootloader or the whole batch)
    /// with custom tracers.
    fn inspect(
        &mut self,
        dispatcher: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs;

    /// Start a new L2 block.
    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv);

    /// Executes the provided transaction with optional bytecode compression using custom tracers.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs);

    /// Execute batch till the end and return the result, with final execution state
    /// and bootloader memory.
    fn finish_batch(&mut self, pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch;
}

/// Extension trait for [`VmInterface`] that provides some additional methods.
pub trait VmInterfaceExt: VmInterface {
    /// Executes the next VM step (either next transaction or bootloader or the whole batch).
    fn execute(&mut self, execution_mode: InspectExecutionMode) -> VmExecutionResultAndLogs {
        self.inspect(&mut <Self::TracerDispatcher>::default(), execution_mode)
    }

    /// Executes a transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        self.inspect_transaction_with_bytecode_compression(
            &mut <Self::TracerDispatcher>::default(),
            tx,
            with_compression,
        )
    }
}

impl<T: VmInterface> VmInterfaceExt for T {}

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

/// VM that tracks decommitment of bytecodes during execution. This is required to create a [`VmDump`].
pub trait VmTrackingContracts: VmInterface {
    /// Returns hashes of all decommitted bytecodes.
    fn used_contract_hashes(&self) -> Vec<H256>;
}
