use std::{collections::HashSet, rc::Rc};

use zksync_types::{bytecode::BytecodeHash, h256_to_u256, Transaction};
use zksync_vm_interface::{pubdata::PubdataBuilder, InspectExecutionMode};

use crate::{
    glue::{history_mode::HistoryMode, GlueInto},
    interface::{
        storage::{StoragePtr, WriteStorage},
        BytecodeCompressionError, BytecodeCompressionResult, FinishedL1Batch, L1BatchEnv,
        L2BlockEnv, PushTransactionResult, SystemEnv, TxExecutionMode, VmExecutionResultAndLogs,
        VmFactory, VmInterface, VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    tracers::old::TracerDispatcher,
    utils::bytecode,
    vm_1_3_2::VmInstance,
};

#[derive(Debug)]
pub struct Vm<S: WriteStorage, H: HistoryMode> {
    pub(crate) vm: VmInstance<S, H::Vm1_3_2Mode>,
    pub(crate) system_env: SystemEnv,
}

impl<S: WriteStorage, H: HistoryMode> Vm<S, H> {
    pub(crate) fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
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
}

impl<S: WriteStorage, H: HistoryMode> VmInterface for Vm<S, H> {
    type TracerDispatcher = TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult<'_> {
        let compressed_bytecodes =
            crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                &mut self.vm,
                &tx,
                self.system_env.execution_mode.glue_into(),
                None,
            );
        PushTransactionResult {
            compressed_bytecodes: compressed_bytecodes.into(),
        }
    }

    fn inspect(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        if let Some(storage_invocations) = tracer.storage_invocations {
            self.vm
                .execution_mode
                .set_invocation_limit(storage_invocations);
        }

        match execution_mode {
            InspectExecutionMode::OneTx => {
                match self.system_env.execution_mode {
                    TxExecutionMode::VerifyExecute => {
                        let enable_call_tracer = tracer
                            .call_tracer.is_some();
                        let result = self.vm.execute_next_tx(
                            self.system_env.default_validation_computational_gas_limit,
                            enable_call_tracer,
                        );
                        if let (Ok(result), Some(call_tracer)) = (&result, &tracer.call_tracer) {
                            call_tracer.set( result.call_traces.clone()).unwrap();

                        }
                        result.glue_into()

                    }
                    TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => self.vm
                        .execute_till_block_end(
                            crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                        )
                        .glue_into(),
                }
            }
            InspectExecutionMode::Bootloader => self.vm.execute_block_tip().glue_into(),
        }
    }

    fn start_new_l2_block(&mut self, _l2_block_env: L2BlockEnv) {
        // Do nothing, because vm 1.3.2 doesn't support L2 blocks
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
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
                let bytecode_hash = BytecodeHash::for_bytecode(bytecode).value();
                let is_known =
                    !deps_hashes.insert(bytecode_hash) || self.vm.is_bytecode_known(&bytecode_hash);

                if is_known {
                    None
                } else {
                    bytecode_hashes.push(bytecode_hash);
                    bytecode::compress(bytecode.clone()).ok()
                }
            });
            compressed_bytecodes = filtered_deps.collect();

            crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                &mut self.vm,
                &tx,
                self.system_env.execution_mode.glue_into(),
                Some(compressed_bytecodes.clone()),
            );
            bytecode_hashes
        } else {
            compressed_bytecodes = vec![];
            crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
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
                    crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                )
                .glue_into(),
        };
        if bytecodes
            .iter()
            .any(|info| !self.vm.is_bytecode_known(info))
        {
            (
                Err(BytecodeCompressionError::BytecodeCompressionFailed),
                result,
            )
        } else {
            (Ok(compressed_bytecodes.into()), result)
        }
    }

    fn finish_batch(&mut self, _pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        self.vm
            .execute_till_block_end(
                crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
            )
            .glue_into()
    }

    fn gas_remaining(&mut self) -> u32 {
        self.vm.gas_remaining()
    }
}

impl<S: WriteStorage, H: HistoryMode> VmFactory<S> for Vm<S, H> {
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

impl<S: WriteStorage> VmInterfaceHistoryEnabled for Vm<S, crate::vm_latest::HistoryEnabled> {
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
