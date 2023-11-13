use crate::interface::{
    BootloaderMemory, BytecodeCompressionError, CurrentExecutionState, FinishedL1Batch, L1BatchEnv,
    L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs, VmInterface,
    VmInterfaceHistoryEnabled, VmMemoryMetrics,
};

use std::collections::HashSet;

use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log};
use zksync_types::Transaction;
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};
use zksync_utils::{h256_to_u256, u256_to_h256};

use crate::glue::history_mode::HistoryMode;
use crate::glue::GlueInto;
use crate::vm_1_3_2::events::merge_events;
use crate::vm_1_3_2::VmInstance;

#[derive(Debug)]
pub struct Vm<S: WriteStorage, H: HistoryMode> {
    pub(crate) vm: VmInstance<S, H::Vm1_3_2Mode>,
    pub(crate) system_env: SystemEnv,
    pub(crate) batch_env: L1BatchEnv,
    pub(crate) last_tx_compressed_bytecodes: Vec<CompressedBytecodeInfo>,
}

impl<S: WriteStorage, H: HistoryMode> VmInterface<S, H> for Vm<S, H> {
    /// Tracers are not supported for vm 1.3.2. So we use `()` as a placeholder
    type TracerDispatcher = ();

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let oracle_tools = crate::vm_1_3_2::OracleTools::new(storage.clone());
        let block_properties = crate::vm_1_3_2::BlockProperties {
            default_aa_code_hash: h256_to_u256(
                system_env.base_system_smart_contracts.default_aa.hash,
            ),
            zkporter_is_available: false,
        };
        let inner_vm: VmInstance<S, H::Vm1_3_2Mode> =
            crate::vm_1_3_2::vm_with_bootloader::init_vm_with_gas_limit(
                oracle_tools,
                batch_env.clone().glue_into(),
                block_properties,
                system_env.execution_mode.glue_into(),
                &system_env.base_system_smart_contracts.clone().glue_into(),
                system_env.gas_limit,
            );
        Self {
            vm: inner_vm,
            system_env,
            batch_env,
            last_tx_compressed_bytecodes: vec![],
        }
    }

    fn push_transaction(&mut self, tx: Transaction) {
        crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
            None,
        )
    }

    fn inspect(
        &mut self,
        _tracer: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        match execution_mode {
            VmExecutionMode::OneTx => {
                match self.system_env.execution_mode {
                    TxExecutionMode::VerifyExecute => {
                        // Even that call tracer is supported here, we don't use it now 
                        self.vm.execute_next_tx(
                            self.system_env.default_validation_computational_gas_limit,
                            false,
                        ).glue_into()
                    }
                    TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self.vm
                        .execute_till_block_end(
                            crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                        )
                        .glue_into(),
                }
            }
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
        let (_full_history, raw_events, l1_messages) = self.vm.state.event_sink.flatten();
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
        let total_log_queries = self.vm.state.event_sink.get_log_queries()
            + self
                .vm
                .state
                .precompiles_processor
                .get_timestamp_history()
                .len()
            + self.vm.state.storage.get_final_log_queries().len();

        let used_contract_hashes = self
            .vm
            .state
            .decommittment_processor
            .known_bytecodes
            .inner()
            .keys()
            .cloned()
            .collect();

        CurrentExecutionState {
            events,
            storage_log_queries: self.vm.state.storage.get_final_log_queries(),
            used_contract_hashes,
            user_l2_to_l1_logs: l2_to_l1_logs,
            system_logs: vec![],
            total_log_queries,
            cycles_used: self.vm.state.local_state.monotonic_cycle_counter,
            // It's not applicable for vm 1.3.2
            deduplicated_events_logs: vec![],
            storage_refunds: vec![],
        }
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        _tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, BytecodeCompressionError> {
        self.last_tx_compressed_bytecodes = vec![];
        let bytecodes = if with_compression {
            let deps = tx.execute.factory_deps.as_deref().unwrap_or_default();
            let mut deps_hashes = HashSet::with_capacity(deps.len());
            let mut bytecode_hashes = vec![];
            let filtered_deps = deps.iter().filter_map(|bytecode| {
                let bytecode_hash = hash_bytecode(bytecode);
                let is_known =
                    !deps_hashes.insert(bytecode_hash) || self.vm.is_bytecode_known(&bytecode_hash);

                if is_known {
                    None
                } else {
                    bytecode_hashes.push(bytecode_hash);
                    CompressedBytecodeInfo::from_original(bytecode.clone()).ok()
                }
            });
            let compressed_bytecodes: Vec<_> = filtered_deps.collect();

            self.last_tx_compressed_bytecodes = compressed_bytecodes.clone();
            crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                &mut self.vm,
                &tx,
                self.system_env.execution_mode.glue_into(),
                Some(compressed_bytecodes),
            );
            bytecode_hashes
        } else {
            crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                &mut self.vm,
                &tx,
                self.system_env.execution_mode.glue_into(),
                Some(vec![]),
            );
            vec![]
        };

        // Even that call tracer is supported here, we don't use it.
        let result = self.vm.execute_next_tx(
            self.system_env.default_validation_computational_gas_limit,
            false,
        );
        if bytecodes
            .iter()
            .any(|info| !self.vm.is_bytecode_known(info))
        {
            Err(crate::interface::BytecodeCompressionError::BytecodeCompressionFailed)
        } else {
            Ok(result.glue_into())
        }
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        VmMemoryMetrics {
            event_sink_inner: self.vm.state.event_sink.get_size(),
            event_sink_history: self.vm.state.event_sink.get_history_size(),
            memory_inner: self.vm.state.memory.get_size(),
            memory_history: self.vm.state.memory.get_history_size(),
            decommittment_processor_inner: self.vm.state.decommittment_processor.get_size(),
            decommittment_processor_history: self
                .vm
                .state
                .decommittment_processor
                .get_history_size(),
            storage_inner: self.vm.state.storage.get_size(),
            storage_history: self.vm.state.storage.get_history_size(),
        }
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        self.vm
            .execute_till_block_end(
                crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
            )
            .glue_into()
    }
}

impl<S: WriteStorage> VmInterfaceHistoryEnabled<S> for Vm<S, crate::vm_latest::HistoryEnabled> {
    fn make_snapshot(&mut self) {
        self.vm.save_current_vm_as_snapshot()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.vm.rollback_to_latest_snapshot_popping();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.vm.pop_snapshot_no_rollback()
    }
}
