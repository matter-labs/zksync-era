use circuit_sequencer_api_1_3_3::sort_storage_access::sort_storage_access_queries;
use itertools::Itertools;
use zk_evm_1_3_1::aux_structures::LogQuery;
use zksync_state::StoragePtr;
use zksync_types::{
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    Transaction, VmVersion,
};
use zksync_utils::{bytecode::CompressedBytecodeInfo, h256_to_u256, u256_to_h256};

use crate::{
    glue::{history_mode::HistoryMode, GlueInto},
    interface::{
        BootloaderMemory, BytecodeCompressionError, CurrentExecutionState, FinishedL1Batch,
        L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode,
        VmExecutionResultAndLogs, VmInterface, VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    vm_m5::{
        events::merge_events,
        storage::Storage,
        vm_instance::{MultiVMSubversion, VmInstance},
    },
};

#[derive(Debug)]
pub struct Vm<S: Storage, H: HistoryMode> {
    pub(crate) vm: VmInstance<S>,
    pub(crate) system_env: SystemEnv,
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) last_tx_compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    _phantom: std::marker::PhantomData<H>,
}

impl<S: Storage, H: HistoryMode> Vm<S, H> {
    pub fn new_with_subversion(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<S>,
        vm_sub_version: MultiVMSubversion,
    ) -> Self {
        let oracle_tools = crate::vm_m5::OracleTools::new(storage.clone(), vm_sub_version);
        let block_properties = zk_evm_1_3_1::block_properties::BlockProperties {
            default_aa_code_hash: h256_to_u256(
                system_env.base_system_smart_contracts.default_aa.hash,
            ),
            zkporter_is_available: false,
        };
        let inner_vm = crate::vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
            vm_sub_version,
            oracle_tools,
            batch_env.clone().glue_into(),
            block_properties,
            system_env.execution_mode.glue_into(),
            &system_env.base_system_smart_contracts.clone().glue_into(),
            system_env.bootloader_gas_limit,
        );
        Self {
            vm: inner_vm,
            system_env,
            batch_env,
            last_tx_compressed_bytecodes: vec![],
            _phantom: Default::default(),
        }
    }
}

impl<S: Storage, H: HistoryMode> VmInterface<S, H> for Vm<S, H> {
    /// Tracers are not supported for here we use `()` as a placeholder
    type TracerDispatcher = ();

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let vm_version: VmVersion = system_env.version.into();
        let vm_sub_version = match vm_version {
            VmVersion::M5WithoutRefunds => MultiVMSubversion::V1,
            VmVersion::M5WithRefunds => MultiVMSubversion::V2,
            _ => panic!("Unsupported protocol version for vm_m5: {:?}", vm_version),
        };
        Self::new_with_subversion(batch_env, system_env, storage, vm_sub_version)
    }

    fn push_transaction(&mut self, tx: Transaction) {
        crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
        )
    }

    fn inspect(
        &mut self,
        _tracer: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        match execution_mode {
            VmExecutionMode::OneTx => match self.system_env.execution_mode {
                TxExecutionMode::VerifyExecute => self.vm.execute_next_tx().glue_into(),
                TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self
                    .vm
                    .execute_till_block_end(
                        crate::vm_m5::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                    )
                    .glue_into(),
            },
            VmExecutionMode::Batch => self.finish_batch().block_tip_execution_result,
            VmExecutionMode::Bootloader => self.vm.execute_block_tip().glue_into(),
        }
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        vec![]
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        self.last_tx_compressed_bytecodes.clone()
    }

    fn start_new_l2_block(&mut self, _l2_block_env: L2BlockEnv) {
        // Do nothing, because vm 1.3.2 doesn't support L2 blocks
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        let (_, raw_events, l1_messages) = self.vm.state.event_sink.flatten();
        let events = merge_events(raw_events)
            .into_iter()
            .map(|e| e.into_vm_event(self.batch_env.number))
            .collect();
        let l2_to_l1_logs = l1_messages
            .into_iter()
            .map(|m| {
                UserL2ToL1Log(L2ToL1Log {
                    shard_id: m.shard_id,
                    is_service: m.is_first,
                    tx_number_in_block: m.tx_number_in_block,
                    sender: m.address,
                    key: u256_to_h256(m.key),
                    value: u256_to_h256(m.value),
                })
            })
            .collect();

        let used_contract_hashes = self
            .vm
            .state
            .decommittment_processor
            .known_bytecodes
            .inner()
            .keys()
            .cloned()
            .collect();

        let storage_log_queries = self.vm.get_final_log_queries();

        // To allow calling the `vm-1.3.3`s. method, the `v1.3.1`'s `LogQuery` has to be converted
        // to the `vm-1.3.3`'s `LogQuery`. Then, we need to convert it back.
        let deduplicated_logs: Vec<LogQuery> = sort_storage_access_queries(
            &storage_log_queries
                .iter()
                .map(|log| {
                    GlueInto::<zk_evm_1_3_3::aux_structures::LogQuery>::glue_into(log.log_query)
                })
                .collect_vec(),
        )
        .1
        .into_iter()
        .map(GlueInto::<zk_evm_1_3_1::aux_structures::LogQuery>::glue_into)
        .collect();

        CurrentExecutionState {
            events,
            deduplicated_storage_logs: deduplicated_logs
                .into_iter()
                .map(GlueInto::glue_into)
                .collect(),
            used_contract_hashes,
            system_logs: vec![],
            user_l2_to_l1_logs: l2_to_l1_logs,
            // Fields below are not produced by `vm5`
            storage_refunds: vec![],
            pubdata_costs: vec![],
        }
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        _tracer: Self::TracerDispatcher,
        tx: Transaction,
        _with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
        );
        (Ok(()), self.execute(VmExecutionMode::OneTx))
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        VmMemoryMetrics {
            event_sink_inner: 0,
            event_sink_history: 0,
            memory_inner: 0,
            memory_history: 0,
            decommittment_processor_inner: 0,
            decommittment_processor_history: 0,
            storage_inner: 0,
            storage_history: 0,
        }
    }

    fn gas_remaining(&self) -> u32 {
        self.vm.gas_remaining()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        self.vm
            .execute_till_block_end(
                crate::vm_m5::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
            )
            .glue_into()
    }
}

impl<S: Storage> VmInterfaceHistoryEnabled<S> for Vm<S, crate::vm_latest::HistoryEnabled> {
    fn make_snapshot(&mut self) {
        self.vm.save_current_vm_as_snapshot()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.vm.rollback_to_latest_snapshot_popping();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.vm.snapshots.pop();
    }
}
