use std::{collections::VecDeque, rc::Rc};

use circuit_sequencer_api::sort_storage_access::sort_storage_access_queries;
use zksync_types::{l2_to_l1_log::UserL2ToL1Log, Transaction};
use zksync_vm_interface::{pubdata::PubdataBuilder, InspectExecutionMode};

use crate::{
    glue::GlueInto,
    interface::{
        storage::{StoragePtr, WriteStorage},
        BytecodeCompressionError, BytecodeCompressionResult, CurrentExecutionState,
        FinishedL1Batch, L1BatchEnv, L2BlockEnv, PushTransactionResult, SystemEnv, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
    },
    utils::glue_log_query,
    vm_latest::HistoryEnabled,
    vm_refunds_enhancement::{
        bootloader_state::BootloaderState,
        old_vm::events::merge_events,
        tracers::dispatcher::TracerDispatcher,
        types::internals::{new_vm_state, VmSnapshot, ZkSyncVmState},
    },
    HistoryMode,
};

/// Main entry point for Virtual Machine integration.
/// The instance should process only one l1 batch
#[derive(Debug)]
pub struct Vm<S: WriteStorage, H: HistoryMode> {
    pub(crate) bootloader_state: BootloaderState,
    // Current state and oracles of virtual machine
    pub(crate) state: ZkSyncVmState<S, H::VmVirtualBlocksRefundsEnhancement>,
    pub(crate) storage: StoragePtr<S>,
    pub(crate) system_env: SystemEnv,
    pub(crate) batch_env: L1BatchEnv,
    // Snapshots for the current run
    pub(crate) snapshots: VecDeque<VmSnapshot>,
    _phantom: std::marker::PhantomData<H>,
}

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(super) fn gas_remaining(&self) -> u32 {
        self.state.local_state.callstack.current.ergs_remaining
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        let (raw_events, l1_messages) = self.state.event_sink.flatten();
        let events: Vec<_> = merge_events(raw_events)
            .into_iter()
            .map(|e| e.into_vm_event(self.batch_env.number))
            .collect();

        let l2_to_l1_logs = l1_messages
            .into_iter()
            .map(|log| UserL2ToL1Log(log.glue_into()))
            .collect();

        let storage_log_queries = self.state.storage.get_final_log_queries();

        let deduped_storage_log_queries = sort_storage_access_queries(
            storage_log_queries
                .iter()
                .map(|log| glue_log_query(log.log_query)),
        )
        .1;

        CurrentExecutionState {
            events,
            deduplicated_storage_logs: deduped_storage_log_queries
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            used_contract_hashes: self.get_used_contracts(),
            user_l2_to_l1_logs: l2_to_l1_logs,
            system_logs: vec![],
            storage_refunds: self.state.storage.returned_refunds.inner().clone(),
            pubdata_costs: Vec::new(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmInterface for Vm<S, H> {
    type TracerDispatcher = TracerDispatcher<S, H::VmVirtualBlocksRefundsEnhancement>;

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_> {
        self.push_transaction_with_compression(tx, true);
        PushTransactionResult {
            compressed_bytecodes: self
                .bootloader_state
                .get_last_tx_compressed_bytecodes()
                .into(),
        }
    }

    /// Execute VM with custom tracers.
    fn inspect(
        &mut self,
        dispatcher: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inspect_inner(dispatcher, execution_mode.into())
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env);
    }

    /// Inspect transaction with optional bytecode compression.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        dispatcher: &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        self.push_transaction_with_compression(tx, with_compression);
        let result = self.inspect(dispatcher, InspectExecutionMode::OneTx);
        if self.has_unpublished_bytecodes() {
            (
                Err(BytecodeCompressionError::BytecodeCompressionFailed),
                result,
            )
        } else {
            (
                Ok(self
                    .bootloader_state
                    .get_last_tx_compressed_bytecodes()
                    .into()),
                result,
            )
        }
    }

    fn finish_batch(&mut self, _pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        let result = self.inspect_inner(&mut TracerDispatcher::default(), VmExecutionMode::Batch);
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self.bootloader_state.bootloader_memory();
        FinishedL1Batch {
            block_tip_execution_result: result,
            final_execution_state: execution_state,
            final_bootloader_memory: Some(bootloader_memory),
            pubdata_input: None,
            state_diffs: None,
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmFactory<S> for Vm<S, H> {
    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let (state, bootloader_state) = new_vm_state(storage.clone(), &system_env, &batch_env);
        Self {
            bootloader_state,
            state,
            storage,
            system_env,
            batch_env,
            snapshots: VecDeque::new(),
            _phantom: Default::default(),
        }
    }
}

impl<S: WriteStorage> VmInterfaceHistoryEnabled for Vm<S, HistoryEnabled> {
    /// Create snapshot of current vm state and push it into the memory
    fn make_snapshot(&mut self) {
        self.make_snapshot_inner();
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let snapshot = self
            .snapshots
            .pop_back()
            .expect("Snapshot should be created before rolling it back");
        self.rollback_to_snapshot(snapshot);
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots.pop_back();
    }

    fn pop_front_snapshot_no_rollback(&mut self) {
        self.snapshots.pop_front();
    }
}
