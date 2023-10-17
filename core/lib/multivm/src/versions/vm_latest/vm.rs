use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::Transaction;
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::vm_latest::old_vm::events::merge_events;
use crate::vm_latest::old_vm::history_recorder::{HistoryEnabled, HistoryMode};

use crate::interface::BytecodeCompressionError;
use crate::interface::{
    BootloaderMemory, CurrentExecutionState, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode,
    VmExecutionResultAndLogs,
};
use crate::vm_latest::bootloader_state::BootloaderState;
use crate::vm_latest::tracers::traits::VmTracer;
use crate::vm_latest::types::internals::{new_vm_state, VmSnapshot, ZkSyncVmState};

/// Main entry point for Virtual Machine integration.
/// The instance should process only one l1 batch
#[derive(Debug)]
pub struct Vm<S: WriteStorage, H: HistoryMode> {
    pub(crate) bootloader_state: BootloaderState,
    // Current state and oracles of virtual machine
    pub(crate) state: ZkSyncVmState<S, H>,
    pub(crate) storage: StoragePtr<S>,
    pub(crate) system_env: SystemEnv,
    pub(crate) batch_env: L1BatchEnv,
    // Snapshots for the current run
    pub(crate) snapshots: Vec<VmSnapshot>,
    _phantom: std::marker::PhantomData<H>,
}

/// Public interface for VM
impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>, _: H) -> Self {
        let (state, bootloader_state) = new_vm_state(storage.clone(), &system_env, &batch_env);
        Self {
            bootloader_state,
            state,
            storage,
            system_env,
            batch_env,
            snapshots: vec![],
            _phantom: Default::default(),
        }
    }

    /// Push tx into memory for the future execution
    pub fn push_transaction(&mut self, tx: Transaction) {
        self.push_transaction_with_compression(tx, true)
    }

    /// Execute VM with default tracers. The execution mode determines whether the VM will stop and
    /// how the vm will be processed.
    pub fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        self.inspect(vec![], execution_mode)
    }

    /// Execute VM with custom tracers.
    pub fn inspect(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<S, H>>>,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inspect_inner(tracers, execution_mode)
    }

    /// Get current state of bootloader memory.
    pub fn get_bootloader_memory(&self) -> BootloaderMemory {
        self.bootloader_state.bootloader_memory()
    }

    /// Get compressed bytecodes of the last executed transaction
    pub fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        self.bootloader_state.get_last_tx_compressed_bytecodes()
    }

    pub fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env);
    }

    /// Get current state of virtual machine.
    /// This method should be used only after the batch execution.
    /// Otherwise it can panic.
    pub fn get_current_execution_state(&self) -> CurrentExecutionState {
        let (_full_history, raw_events, l1_messages) = self.state.event_sink.flatten();
        let events = merge_events(raw_events)
            .into_iter()
            .map(|e| e.into_vm_event(self.batch_env.number))
            .collect();
        let l2_to_l1_logs = l1_messages.into_iter().map(|log| log.into()).collect();
        let total_log_queries = self.state.event_sink.get_log_queries()
            + self
                .state
                .precompiles_processor
                .get_timestamp_history()
                .len()
            + self.state.storage.get_final_log_queries().len();

        CurrentExecutionState {
            events,
            storage_log_queries: self.state.storage.get_final_log_queries(),
            used_contract_hashes: self.get_used_contracts(),
            l2_to_l1_logs,
            total_log_queries,
            cycles_used: self.state.local_state.monotonic_cycle_counter,
            storage_refunds: self.state.storage.returned_refunds.inner().clone(),
        }
    }

    /// Execute transaction with optional bytecode compression.
    pub fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, BytecodeCompressionError> {
        self.inspect_transaction_with_bytecode_compression(vec![], tx, with_compression)
    }

    /// Inspect transaction with optional bytecode compression.
    pub fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<S, H>>>,
        tx: Transaction,
        with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, BytecodeCompressionError> {
        self.push_transaction_with_compression(tx, with_compression);
        let result = self.inspect(tracers, VmExecutionMode::OneTx);
        if self.has_unpublished_bytecodes() {
            Err(BytecodeCompressionError::BytecodeCompressionFailed)
        } else {
            Ok(result)
        }
    }
}

/// Methods of vm, which required some history manipullations
impl<S: WriteStorage> Vm<S, HistoryEnabled> {
    /// Create snapshot of current vm state and push it into the memory
    pub fn make_snapshot(&mut self) {
        self.make_snapshot_inner()
    }

    /// Rollback vm state to the latest snapshot and destroy the snapshot
    pub fn rollback_to_the_latest_snapshot(&mut self) {
        let snapshot = self
            .snapshots
            .pop()
            .expect("Snapshot should be created before rolling it back");
        self.rollback_to_snapshot(snapshot);
    }

    /// Pop the latest snapshot from the memory and destroy it
    pub fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots
            .pop()
            .expect("Snapshot should be created before rolling it back");
    }
}
