use circuit_sequencer_api_1_4_2::sort_storage_access::sort_storage_access_queries;
use zksync_types::{
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    Transaction,
};

use crate::{
    glue::GlueInto,
    interface::{
        storage::{StoragePtr, WriteStorage},
        BytecodeCompressionError, BytecodeCompressionResult, CurrentExecutionState,
        FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
        VmMemoryMetrics,
    },
    utils::events::extract_l2tol1logs_from_l1_messenger,
    vm_1_4_2::{
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
    pub(crate) state: ZkSyncVmState<S, H::Vm1_4_2>,
    pub(crate) storage: StoragePtr<S>,
    pub(crate) system_env: SystemEnv,
    pub(crate) batch_env: L1BatchEnv,
    // Snapshots for the current run
    pub(crate) snapshots: Vec<VmSnapshot>,
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

        let user_l2_to_l1_logs = extract_l2tol1logs_from_l1_messenger(&events);
        let system_logs = l1_messages
            .into_iter()
            .map(|log| SystemL2ToL1Log(log.glue_into()))
            .collect();

        let storage_log_queries = self.state.storage.get_final_log_queries();

        let deduped_storage_log_queries =
            sort_storage_access_queries(storage_log_queries.iter().map(|log| &log.log_query)).1;

        CurrentExecutionState {
            events,
            deduplicated_storage_logs: deduped_storage_log_queries
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            used_contract_hashes: self.get_used_contracts(),
            user_l2_to_l1_logs: user_l2_to_l1_logs
                .into_iter()
                .map(|log| UserL2ToL1Log(log.into()))
                .collect(),
            system_logs,
            storage_refunds: self.state.storage.returned_refunds.inner().clone(),
            pubdata_costs: Vec::new(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmInterface for Vm<S, H> {
    type TracerDispatcher = TracerDispatcher<S, H::Vm1_4_2>;

    /// Push tx into memory for the future execution
    fn push_transaction(&mut self, tx: Transaction) {
        self.push_transaction_with_compression(tx, true);
    }

    /// Execute VM with custom tracers.
    fn inspect(
        &mut self,
        tracer: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inspect_inner(tracer, execution_mode, None)
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.bootloader_state.start_new_l2_block(l2_block_env);
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        self.push_transaction_with_compression(tx, with_compression);
        let result = self.inspect_inner(tracer, VmExecutionMode::OneTx, None);
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

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        self.record_vm_memory_metrics_inner()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        let result = self.inspect(TracerDispatcher::default(), VmExecutionMode::Batch);
        let execution_state = self.get_current_execution_state();
        let bootloader_memory = self.bootloader_state.bootloader_memory();
        FinishedL1Batch {
            block_tip_execution_result: result,
            final_execution_state: execution_state,
            final_bootloader_memory: Some(bootloader_memory),
            pubdata_input: Some(
                self.bootloader_state
                    .get_pubdata_information()
                    .clone()
                    .build_pubdata(false),
            ),
            state_diffs: Some(
                self.bootloader_state
                    .get_pubdata_information()
                    .state_diffs
                    .clone(),
            ),
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
            snapshots: vec![],
            _phantom: Default::default(),
        }
    }
}

impl<S: WriteStorage> VmInterfaceHistoryEnabled for Vm<S, crate::vm_latest::HistoryEnabled> {
    fn make_snapshot(&mut self) {
        self.make_snapshot_inner();
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        let snapshot = self
            .snapshots
            .pop()
            .expect("Snapshot should be created before rolling it back");
        self.rollback_to_snapshot(snapshot);
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.snapshots.pop();
    }
}
