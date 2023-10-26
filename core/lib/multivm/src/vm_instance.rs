use crate::interface::{
    FinishedL1Batch, L2BlockEnv, SystemEnv, TxExecutionMode, VmExecutionMode, VmInterface,
    VmInterfaceHistoryEnabled, VmMemoryMetrics,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use zksync_state::{ReadStorage, StorageView};
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};

use crate::glue::history_mode::HistoryMode;
use crate::glue::tracers::MultivmTracer;
use crate::glue::GlueInto;

pub struct VmInstance<S: ReadStorage, H: HistoryMode> {
    pub(crate) vm: VmInstanceVersion<S, H>,
    pub(crate) system_env: SystemEnv,
    pub(crate) last_tx_compressed_bytecodes: Vec<CompressedBytecodeInfo>,
}

#[derive(Debug)]
pub(crate) enum VmInstanceVersion<S: ReadStorage, H: HistoryMode> {
    VmM5(Box<crate::vm_m5::VmInstance<StorageView<S>>>),
    VmM6(Box<crate::vm_m6::VmInstance<StorageView<S>, H::VmM6Mode>>),
    Vm1_3_2(Box<crate::vm_1_3_2::VmInstance<StorageView<S>, H::Vm1_3_2Mode>>),
    VmVirtualBlocks(Box<crate::vm_virtual_blocks::Vm<StorageView<S>, H>>),
    VmVirtualBlocksRefundsEnhancement(Box<crate::vm_latest::Vm<StorageView<S>, H>>),
}

impl<S: ReadStorage, H: HistoryMode> VmInstance<S, H> {
    /// Push tx into memory for the future execution
    pub fn push_transaction(&mut self, tx: &zksync_types::Transaction) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => {
                crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    self.system_env.execution_mode.glue_into(),
                )
            }
            VmInstanceVersion::VmM6(vm) => {
                crate::vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    self.system_env.execution_mode.glue_into(),
                    None,
                )
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    self.system_env.execution_mode.glue_into(),
                    None,
                )
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.push_transaction(tx.clone());
            }
        }
    }

    /// Return the results of execution of all batch
    pub fn finish_batch(&mut self) -> FinishedL1Batch {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm
                .execute_till_block_end(
                    crate::vm_m5::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
                )
                .glue_into(),
            VmInstanceVersion::VmM6(vm) => vm
                .execute_till_block_end(
                    crate::vm_m6::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
                )
                .glue_into(),
            VmInstanceVersion::Vm1_3_2(vm) => vm
                .execute_till_block_end(
                    crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
                )
                .glue_into(),
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                let result = vm.execute(VmExecutionMode::Batch.glue_into());
                let execution_state = vm.get_current_execution_state();
                let bootloader_memory = vm.get_bootloader_memory();
                FinishedL1Batch {
                    block_tip_execution_result: result.glue_into(),
                    final_execution_state: execution_state.glue_into(),
                    final_bootloader_memory: Some(bootloader_memory),
                }
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                let result = vm.execute(VmExecutionMode::Batch);
                let execution_state = vm.get_current_execution_state();
                let bootloader_memory = vm.get_bootloader_memory();
                FinishedL1Batch {
                    block_tip_execution_result: result,
                    final_execution_state: execution_state,
                    final_bootloader_memory: Some(bootloader_memory),
                }
            }
        }
    }

    /// Execute the batch without stops after each tx.
    /// This method allows to execute the part  of the VM cycle after executing all txs.
    pub fn execute_block_tip(&mut self) -> crate::interface::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.execute_block_tip().glue_into(),
            VmInstanceVersion::VmM6(vm) => vm.execute_block_tip().glue_into(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.execute_block_tip().glue_into(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm
                .execute(VmExecutionMode::Bootloader.glue_into())
                .glue_into(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.execute(VmExecutionMode::Bootloader)
            }
        }
    }

    /// Execute next transaction and stop vm right after next transaction execution
    pub fn execute_next_transaction(&mut self) -> crate::interface::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => match self.system_env.execution_mode {
                TxExecutionMode::VerifyExecute => vm.execute_next_tx().glue_into(),
                TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => vm
                    .execute_till_block_end(
                        crate::vm_m5::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                    )
                    .glue_into(),
            },
            VmInstanceVersion::VmM6(vm) => {
                match self.system_env.execution_mode {
                    TxExecutionMode::VerifyExecute => {
                        // Even that call tracer is supported by vm vm1.3.2, we don't use it for multivm
                        vm.execute_next_tx(
                            self.system_env.default_validation_computational_gas_limit,
                            false,
                        )
                        .glue_into()
                    }
                    TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => vm
                        .execute_till_block_end(
                             crate::vm_m6::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                        )
                        .glue_into(),
                }
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                match self.system_env.execution_mode {
                    TxExecutionMode::VerifyExecute => {
                        // Even that call tracer is supported by vm vm1.3.2, we don't use it for multivm
                        vm.execute_next_tx(
                            self.system_env.default_validation_computational_gas_limit,
                            false,
                        )
                        .glue_into()
                    }
                    TxExecutionMode::EstimateFee | TxExecutionMode::EthCall => vm
                        .execute_till_block_end(
                            crate::vm_1_3_2::vm_with_bootloader::BootloaderJobType::TransactionExecution,
                        )
                        .glue_into(),
                }
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.execute(VmExecutionMode::OneTx.glue_into()).glue_into()
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.execute(VmExecutionMode::OneTx)
            }
        }
    }

    /// Get compressed bytecodes of the last executed transaction
    pub fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        match &self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.get_last_tx_compressed_bytecodes(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.get_last_tx_compressed_bytecodes()
            }
            _ => self.last_tx_compressed_bytecodes.clone(),
        }
    }

    /// Execute next transaction with custom tracers
    pub fn inspect_next_transaction(
        &mut self,
        tracers: Vec<Box<dyn MultivmTracer<StorageView<S>, H>>>,
    ) -> crate::interface::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                let dispatcher = crate::vm_virtual_blocks::TracerDispatcher {
                    tracers: tracers.into_iter().map(|t| t.vm_virtual_blocks()).collect(),
                };
                vm.inspect(dispatcher, VmExecutionMode::OneTx)
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                let dispatcher = crate::vm_latest::TracerDispatcher {
                    tracers: tracers.into_iter().map(|t| t.latest()).collect(),
                };
                vm.inspect(dispatcher, VmExecutionMode::OneTx)
            }
            _ => self.execute_next_transaction(),
        }
    }

    /// Execute transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> Result<
        crate::interface::VmExecutionResultAndLogs,
        crate::interface::BytecodeCompressionError,
    > {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => {
                crate::vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    &tx,
                    self.system_env.execution_mode.glue_into(),
                );
                Ok(vm.execute_next_tx().glue_into())
            }
            VmInstanceVersion::VmM6(vm) => {
                use crate::vm_m6::storage::Storage;
                let bytecodes = if with_compression {
                    let deps = tx.execute.factory_deps.as_deref().unwrap_or_default();
                    let mut deps_hashes = HashSet::with_capacity(deps.len());
                    let mut bytecode_hashes = vec![];
                    let filtered_deps = deps.iter().filter_map(|bytecode| {
                        let bytecode_hash = hash_bytecode(bytecode);
                        let is_known = !deps_hashes.insert(bytecode_hash)
                            || vm
                                .state
                                .storage
                                .storage
                                .get_ptr()
                                .borrow_mut()
                                .is_bytecode_exists(&bytecode_hash);

                        if is_known {
                            None
                        } else {
                            bytecode_hashes.push(bytecode_hash);
                            CompressedBytecodeInfo::from_original(bytecode.clone()).ok()
                        }
                    });
                    let compressed_bytecodes: Vec<_> = filtered_deps.collect();

                    self.last_tx_compressed_bytecodes = compressed_bytecodes.clone();
                    crate::vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(compressed_bytecodes),
                    );
                    bytecode_hashes
                } else {
                    crate::vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(vec![]),
                    );
                    vec![]
                };

                // Even that call tracer is supported by vm m6, we don't use it for multivm
                let result = vm
                    .execute_next_tx(
                        self.system_env.default_validation_computational_gas_limit,
                        false,
                    )
                    .glue_into();
                if bytecodes.iter().any(|info| {
                    !vm.state
                        .storage
                        .storage
                        .get_ptr()
                        .borrow_mut()
                        .is_bytecode_exists(info)
                }) {
                    Err(crate::interface::BytecodeCompressionError::BytecodeCompressionFailed)
                } else {
                    Ok(result)
                }
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                use crate::vm_m6::storage::Storage;
                let bytecodes = if with_compression {
                    let deps = tx.execute.factory_deps.as_deref().unwrap_or_default();
                    let mut deps_hashes = HashSet::with_capacity(deps.len());
                    let mut bytecode_hashes = vec![];
                    let filtered_deps = deps.iter().filter_map(|bytecode| {
                        let bytecode_hash = hash_bytecode(bytecode);
                        let is_known = !deps_hashes.insert(bytecode_hash)
                            || vm
                                .state
                                .storage
                                .storage
                                .get_ptr()
                                .borrow_mut()
                                .is_bytecode_exists(&bytecode_hash);

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
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(compressed_bytecodes),
                    );
                    bytecode_hashes
                } else {
                    crate::vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(vec![]),
                    );
                    vec![]
                };

                // Even that call tracer is supported by vm m6, we don't use it for multivm
                let result = vm
                    .execute_next_tx(
                        self.system_env.default_validation_computational_gas_limit,
                        false,
                    )
                    .glue_into();
                if bytecodes.iter().any(|info| {
                    !vm.state
                        .storage
                        .storage
                        .get_ptr()
                        .borrow_mut()
                        .is_bytecode_exists(info)
                }) {
                    Err(crate::interface::BytecodeCompressionError::BytecodeCompressionFailed)
                } else {
                    Ok(result)
                }
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => vm
                .execute_transaction_with_bytecode_compression(tx, with_compression)
                .glue_into(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
        }
    }

    /// Inspect transaction with optional bytecode compression.
    pub fn inspect_transaction_with_bytecode_compression<T: MultivmTracer<StorageView<S>, H>>(
        &mut self,
        tracers: Vec<Box<dyn MultivmTracer<StorageView<S>, H>>>,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> Result<
        crate::interface::VmExecutionResultAndLogs,
        crate::interface::BytecodeCompressionError,
    > {
        match &mut self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                let dispatcher = crate::vm_virtual_blocks::TracerDispatcher {
                    tracers: tracers.into_iter().map(|t| t.vm_virtual_blocks()).collect(),
                };

                vm.inspect_transaction_with_bytecode_compression(dispatcher, tx, with_compression)
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                let dispatcher = crate::vm_latest::TracerDispatcher {
                    tracers: tracers.into_iter().map(|t| t.latest()).collect(),
                };

                vm.inspect_transaction_with_bytecode_compression(dispatcher, tx, with_compression)
            }
            _ => {
                self.last_tx_compressed_bytecodes = vec![];
                self.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
        }
    }

    pub fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        match &mut self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.start_new_l2_block(l2_block_env.glue_into());
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            _ => {}
        }
    }

    pub fn record_vm_memory_metrics(&self) -> Option<VmMemoryMetrics> {
        match &self.vm {
            VmInstanceVersion::VmM5(_) => None,
            VmInstanceVersion::VmM6(vm) => Some(VmMemoryMetrics {
                event_sink_inner: vm.state.event_sink.get_size(),
                event_sink_history: vm.state.event_sink.get_history_size(),
                memory_inner: vm.state.memory.get_size(),
                memory_history: vm.state.memory.get_history_size(),
                decommittment_processor_inner: vm.state.decommittment_processor.get_size(),
                decommittment_processor_history: vm
                    .state
                    .decommittment_processor
                    .get_history_size(),
                storage_inner: vm.state.storage.get_size(),
                storage_history: vm.state.storage.get_history_size(),
            }),
            VmInstanceVersion::Vm1_3_2(vm) => Some(VmMemoryMetrics {
                event_sink_inner: vm.state.event_sink.get_size(),
                event_sink_history: vm.state.event_sink.get_history_size(),
                memory_inner: vm.state.memory.get_size(),
                memory_history: vm.state.memory.get_history_size(),
                decommittment_processor_inner: vm.state.decommittment_processor.get_size(),
                decommittment_processor_history: vm
                    .state
                    .decommittment_processor
                    .get_history_size(),
                storage_inner: vm.state.storage.get_size(),
                storage_history: vm.state.storage.get_history_size(),
            }),
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                Some(vm.record_vm_memory_metrics().glue_into())
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                Some(vm.record_vm_memory_metrics())
            }
        }
    }
}

impl<S: ReadStorage> VmInstance<S, crate::vm_latest::HistoryEnabled> {
    pub fn make_snapshot(&mut self) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.save_current_vm_as_snapshot(),
            VmInstanceVersion::VmM6(vm) => vm.save_current_vm_as_snapshot(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.save_current_vm_as_snapshot(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.make_snapshot(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => vm.make_snapshot(),
        }
    }

    pub fn rollback_to_the_latest_snapshot(&mut self) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.rollback_to_latest_snapshot_popping(),
            VmInstanceVersion::VmM6(vm) => vm.rollback_to_latest_snapshot_popping(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.rollback_to_latest_snapshot_popping(),
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.rollback_to_the_latest_snapshot();
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.rollback_to_the_latest_snapshot();
            }
        }
    }

    pub fn pop_snapshot_no_rollback(&mut self) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => {
                // A dedicated method was added later.
                vm.snapshots.pop().unwrap();
            }
            VmInstanceVersion::VmM6(vm) => vm.pop_snapshot_no_rollback(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.pop_snapshot_no_rollback(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.pop_snapshot_no_rollback(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.pop_snapshot_no_rollback()
            }
        }
    }
}
