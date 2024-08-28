use std::collections::HashSet;

use zksync_types::{vm::VmVersion, Transaction};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::{
    glue::{history_mode::HistoryMode, GlueInto},
    interface::{
        storage::StoragePtr, BytecodeCompressionError, BytecodeCompressionResult, FinishedL1Batch,
        L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
        VmMemoryMetrics,
    },
    tracers::old::TracerDispatcher,
    utils::bytecode,
    vm_m6::{storage::Storage, vm_instance::MultiVMSubversion, VmInstance},
};

#[derive(Debug)]
pub struct Vm<S: Storage, H: HistoryMode> {
    pub(crate) vm: VmInstance<S, H::VmM6Mode>,
    pub(crate) system_env: SystemEnv,
}

impl<S: Storage, H: HistoryMode> Vm<S, H> {
    pub fn new_with_subversion(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<S>,
        vm_sub_version: MultiVMSubversion,
    ) -> Self {
        let oracle_tools = crate::vm_m6::OracleTools::new(storage.clone(), H::VmM6Mode::default());
        let block_properties = zk_evm_1_3_1::block_properties::BlockProperties {
            default_aa_code_hash: h256_to_u256(
                system_env.base_system_smart_contracts.default_aa.hash,
            ),
            zkporter_is_available: false,
        };
        let inner_vm = crate::vm_m6::vm_with_bootloader::init_vm_with_gas_limit(
            vm_sub_version,
            oracle_tools,
            batch_env.glue_into(),
            block_properties,
            system_env.execution_mode.glue_into(),
            &system_env.base_system_smart_contracts.clone().glue_into(),
            system_env.bootloader_gas_limit,
        );
        Self {
            vm: inner_vm,
            system_env,
        }
    }
}

impl<S: Storage, H: HistoryMode> VmInterface for Vm<S, H> {
    type TracerDispatcher = TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) {
        crate::vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
            &mut self.vm,
            &tx,
            self.system_env.execution_mode.glue_into(),
            None,
        )
    }

    fn inspect(
        &mut self,
        tracer: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        if let Some(storage_invocations) = tracer.storage_invocations {
            self.vm
                .execution_mode
                .set_invocation_limit(storage_invocations);
        }

        match execution_mode {
            VmExecutionMode::OneTx => match self.system_env.execution_mode {
                TxExecutionMode::VerifyExecute => {
                    let enable_call_tracer = tracer.call_tracer.is_some();
                    let result = self.vm.execute_next_tx(
                        self.system_env.default_validation_computational_gas_limit,
                        enable_call_tracer,
                    );
                    if let (Ok(result), Some(call_tracer)) = (&result, &tracer.call_tracer) {
                        call_tracer.set(result.call_traces.clone()).unwrap();
                    }
                    result.glue_into()
                }
                TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self
                    .vm
                    .execute_till_block_end(
                        crate::vm_m6::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                    )
                    .glue_into(),
            },
            VmExecutionMode::Batch => self.finish_batch().block_tip_execution_result,
            VmExecutionMode::Bootloader => self.vm.execute_block_tip().glue_into(),
        }
    }

    fn start_new_l2_block(&mut self, _l2_block_env: L2BlockEnv) {
        // Do nothing, because vm 1.3.2 doesn't support L2 blocks
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult, VmExecutionResultAndLogs) {
        if let Some(storage_invocations) = tracer.storage_invocations {
            self.vm
                .execution_mode
                .set_invocation_limit(storage_invocations);
        }

        let compressed_bytecodes: Vec<_>;
        let bytecodes = if with_compression {
            let deps = &tx.execute.factory_deps;
            let mut deps_hashes = HashSet::with_capacity(deps.len());
            let mut bytecode_hashes = vec![];
            let filtered_deps = deps.iter().filter_map(|bytecode| {
                let bytecode_hash = hash_bytecode(bytecode);
                let is_known = !deps_hashes.insert(bytecode_hash)
                    || self.vm.is_bytecode_exists(&bytecode_hash);

                if is_known {
                    None
                } else {
                    bytecode_hashes.push(bytecode_hash);
                    bytecode::compress(bytecode.clone()).ok()
                }
            });
            compressed_bytecodes = filtered_deps.collect();

            crate::vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                &mut self.vm,
                &tx,
                self.system_env.execution_mode.glue_into(),
                Some(compressed_bytecodes.clone()),
            );
            bytecode_hashes
        } else {
            compressed_bytecodes = vec![];
            crate::vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                &mut self.vm,
                &tx,
                self.system_env.execution_mode.glue_into(),
                Some(vec![]),
            );
            vec![]
        };

        // Even that call tracer is supported here, we don't use it.
        let result = match self.system_env.execution_mode {
            TxExecutionMode::VerifyExecute => {
                let enable_call_tracer = tracer.call_tracer.is_some();
                let result = self.vm.execute_next_tx(
                    self.system_env.default_validation_computational_gas_limit,
                    enable_call_tracer,
                );
                if let (Ok(result), Some(call_tracer)) = (&result, &tracer.call_tracer) {
                    call_tracer.set(result.call_traces.clone()).unwrap();
                }
                result.glue_into()
            }
            TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self
                .vm
                .execute_till_block_end(
                    crate::vm_m6::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                )
                .glue_into(),
        };
        if bytecodes
            .iter()
            .any(|info| !self.vm.is_bytecode_exists(info))
        {
            (
                Err(BytecodeCompressionError::BytecodeCompressionFailed),
                result,
            )
        } else {
            (Ok(compressed_bytecodes), result)
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

    fn gas_remaining(&self) -> u32 {
        self.vm.gas_remaining()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        self.vm
            .execute_till_block_end(
                crate::vm_m6::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
            )
            .glue_into()
    }
}

impl<S: Storage, H: HistoryMode> VmFactory<S> for Vm<S, H> {
    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage: StoragePtr<S>) -> Self {
        let vm_version: VmVersion = system_env.version.into();
        let vm_sub_version = match vm_version {
            VmVersion::M6Initial => MultiVMSubversion::V1,
            VmVersion::M6BugWithCompressionFixed => MultiVMSubversion::V2,
            _ => panic!("Unsupported protocol version for vm_m6: {:?}", vm_version),
        };
        Self::new_with_subversion(batch_env, system_env, storage, vm_sub_version)
    }
}

impl<S: Storage> VmInterfaceHistoryEnabled for Vm<S, crate::vm_latest::HistoryEnabled> {
    fn make_snapshot(&mut self) {
        self.vm.save_current_vm_as_snapshot()
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.vm.rollback_to_latest_snapshot_popping();
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.vm.pop_snapshot_no_rollback();
    }
}
