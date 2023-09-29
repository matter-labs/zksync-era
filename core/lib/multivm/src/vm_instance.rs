use std::collections::HashSet;

use vm_m6::storage::Storage;
use vm_virtual_blocks::{FinishedL1Batch, L2BlockEnv, SystemEnv, VmMemoryMetrics, VmTracer};

use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::VmVersion;
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};
use zksync_utils::h256_to_u256;

use crate::glue::history_mode::HistoryMode;
use crate::glue::GlueInto;
use crate::{BlockProperties, OracleTools};

pub struct VmInstance<'a, S: ReadStorage, H: HistoryMode> {
    pub(crate) vm: VmInstanceVersion<'a, S, H>,
    pub(crate) system_env: SystemEnv,
    pub(crate) last_tx_compressed_bytecodes: Vec<CompressedBytecodeInfo>,
}

#[derive(Debug)]
pub(crate) enum VmInstanceVersion<'a, S: ReadStorage, H: HistoryMode> {
    VmM5(Box<vm_m5::VmInstance<'a, StorageView<S>>>),
    VmM6(Box<vm_m6::VmInstance<'a, StorageView<S>, H::VmM6Mode>>),
    Vm1_3_2(Box<vm_1_3_2::VmInstance<StorageView<S>, H::Vm1_3_2Mode>>),
    VmVirtualBlocks(Box<vm_virtual_blocks::Vm<StorageView<S>, H::VmVirtualBlocksMode>>),
}

impl<'a, S: ReadStorage, H: HistoryMode> VmInstance<'a, S, H> {
    /// Push tx into memory for the future execution
    pub fn push_transaction(&mut self, tx: &zksync_types::Transaction) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => {
                vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    self.system_env.execution_mode.glue_into(),
                )
            }
            VmInstanceVersion::VmM6(vm) => {
                vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    self.system_env.execution_mode.glue_into(),
                    None,
                )
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    self.system_env.execution_mode.glue_into(),
                    None,
                )
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.push_transaction(tx.clone());
            }
        }
    }

    /// Return the results of execution of all batch
    pub fn finish_batch(&mut self) -> FinishedL1Batch {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm
                .execute_till_block_end(
                    vm_m5::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
                )
                .glue_into(),
            VmInstanceVersion::VmM6(vm) => vm
                .execute_till_block_end(
                    vm_m6::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
                )
                .glue_into(),
            VmInstanceVersion::Vm1_3_2(vm) => vm
                .execute_till_block_end(
                    vm_1_3_2::vm_with_bootloader::BootloaderJobType::BlockPostprocessing,
                )
                .glue_into(),
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                let result = vm.execute_the_rest_of_the_batch();
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
    pub fn execute_block_tip(&mut self) -> vm_virtual_blocks::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.execute_block_tip().glue_into(),
            VmInstanceVersion::VmM6(vm) => vm.execute_block_tip().glue_into(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.execute_block_tip().glue_into(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.execute_block_tip(),
        }
    }

    /// Execute next transaction and stop vm right after next transaction execution
    pub fn execute_next_transaction(&mut self) -> vm_virtual_blocks::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.execute_next_tx().glue_into(),
            VmInstanceVersion::VmM6(vm) => {
                vm
                    // even that call tracer is supported by vm m6, we don't use it for multivm
                    .execute_next_tx(
                        self.system_env.default_validation_computational_gas_limit,
                        false,
                    )
                    .glue_into()
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                vm
                    // even that call tracer is supported by vm vm1.3.2, we don't use it for multivm
                    .execute_next_tx(
                        self.system_env.default_validation_computational_gas_limit,
                        false,
                    )
                    .glue_into()
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.execute_next_transaction(),
        }
    }

    /// Get compressed bytecodes of the last executed transaction
    pub fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        match &self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.get_last_tx_compressed_bytecodes(),
            _ => self.last_tx_compressed_bytecodes.clone(),
        }
    }

    /// Execute next transaction with custom tracers
    pub fn inspect_next_transaction(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<StorageView<S>, H::VmVirtualBlocksMode>>>,
    ) -> vm_virtual_blocks::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.inspect_next_transaction(tracers),
            _ => self.execute_next_transaction(),
        }
    }

    /// Execute transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> Result<
        vm_virtual_blocks::VmExecutionResultAndLogs,
        vm_virtual_blocks::BytecodeCompressionError,
    > {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => {
                vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    &tx,
                    self.system_env.execution_mode.glue_into(),
                );
                Ok(vm.execute_next_tx().glue_into())
            }
            VmInstanceVersion::VmM6(vm) => {
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
                    vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(compressed_bytecodes),
                    );
                    bytecode_hashes
                } else {
                    vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(vec![]),
                    );
                    vec![]
                };

                let result = vm
                    // even that call tracer is supported by vm m6, we don't use it for multivm
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
                    Err(vm_virtual_blocks::BytecodeCompressionError::BytecodeCompressionFailed)
                } else {
                    Ok(result)
                }
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
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
                    vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(compressed_bytecodes),
                    );
                    bytecode_hashes
                } else {
                    vm_1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                        vm,
                        &tx,
                        self.system_env.execution_mode.glue_into(),
                        Some(vec![]),
                    );
                    vec![]
                };

                let result = vm
                    // even that call tracer is supported by vm m6, we don't use it for multivm
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
                    Err(vm_virtual_blocks::BytecodeCompressionError::BytecodeCompressionFailed)
                } else {
                    Ok(result)
                }
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
        }
    }

    /// Inspect transaction with optional bytecode compression.
    pub fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracers: Vec<Box<dyn VmTracer<StorageView<S>, H::VmVirtualBlocksMode>>>,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> Result<
        vm_virtual_blocks::VmExecutionResultAndLogs,
        vm_virtual_blocks::BytecodeCompressionError,
    > {
        if let VmInstanceVersion::VmVirtualBlocks(vm) = &mut self.vm {
            vm.inspect_transaction_with_bytecode_compression(tracers, tx, with_compression)
        } else {
            self.last_tx_compressed_bytecodes = vec![];
            self.execute_transaction_with_bytecode_compression(tx, with_compression)
        }
    }

    pub fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        if let VmInstanceVersion::VmVirtualBlocks(vm) = &mut self.vm {
            vm.start_new_l2_block(l2_block_env);
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
            VmInstanceVersion::VmVirtualBlocks(vm) => Some(vm.record_vm_memory_metrics()),
        }
    }
}

pub struct M5NecessaryData<S: ReadStorage, H: HistoryMode> {
    pub oracle_tools: OracleTools<S, H>,
    pub block_properties: BlockProperties,
    pub sub_version: vm_m5::vm::MultiVMSubversion,
}

pub struct M6NecessaryData<S: ReadStorage, H: HistoryMode> {
    pub oracle_tools: OracleTools<S, H>,
    pub block_properties: BlockProperties,
    pub sub_version: vm_m6::vm::MultiVMSubversion,
}

pub struct Vm1_3_2NecessaryData<S: ReadStorage, H: HistoryMode> {
    pub storage_view: StoragePtr<StorageView<S>>,
    pub history_mode: H,
}

pub struct VmVirtualBlocksNecessaryData<S: ReadStorage, H: HistoryMode> {
    pub storage_view: zksync_state::StoragePtr<StorageView<S>>,
    pub history_mode: H,
}

pub enum VmInstanceData<S: ReadStorage, H: HistoryMode> {
    M5(M5NecessaryData<S, H>),
    M6(M6NecessaryData<S, H>),
    Vm1_3_2(Vm1_3_2NecessaryData<S, H>),
    VmVirtualBlocks(VmVirtualBlocksNecessaryData<S, H>),
}

impl<S: ReadStorage, H: HistoryMode> VmInstanceData<S, H> {
    fn m5(
        oracle_tools: OracleTools<S, H>,
        block_properties: BlockProperties,
        sub_version: vm_m5::vm::MultiVMSubversion,
    ) -> Self {
        Self::M5(M5NecessaryData {
            oracle_tools,
            block_properties,
            sub_version,
        })
    }
    fn m6(
        oracle_tools: OracleTools<S, H>,
        block_properties: BlockProperties,
        sub_version: vm_m6::vm::MultiVMSubversion,
    ) -> Self {
        Self::M6(M6NecessaryData {
            oracle_tools,
            block_properties,
            sub_version,
        })
    }

    fn vm_virtual_blocks(storage_view: StoragePtr<StorageView<S>>, history_mode: H) -> Self {
        Self::VmVirtualBlocks(VmVirtualBlocksNecessaryData {
            storage_view,
            history_mode,
        })
    }

    fn vm1_3_2(storage_view: StoragePtr<StorageView<S>>, history_mode: H) -> Self {
        Self::Vm1_3_2(Vm1_3_2NecessaryData {
            storage_view,
            history_mode,
        })
    }

    pub fn new(
        storage_view: StoragePtr<StorageView<S>>,
        system_env: &SystemEnv,
        history: H,
    ) -> Self {
        let protocol_version = system_env.version;
        let vm_version: VmVersion = protocol_version.into();
        Self::new_for_specific_vm_version(storage_view, system_env, history, vm_version)
    }

    // In api we support only subset of vm versions, so we need to create vm instance for specific version
    pub fn new_for_specific_vm_version(
        storage_view: StoragePtr<StorageView<S>>,
        system_env: &SystemEnv,
        history: H,
        vm_version: VmVersion,
    ) -> Self {
        match vm_version {
            VmVersion::M5WithoutRefunds => {
                let oracle_tools = OracleTools::new(vm_version, storage_view, history);
                let block_properties = BlockProperties::new(
                    vm_version,
                    h256_to_u256(system_env.base_system_smart_contracts.default_aa.hash),
                );
                VmInstanceData::m5(
                    oracle_tools,
                    block_properties,
                    vm_m5::vm::MultiVMSubversion::V1,
                )
            }
            VmVersion::M5WithRefunds => {
                let oracle_tools = OracleTools::new(vm_version, storage_view, history);
                let block_properties = BlockProperties::new(
                    vm_version,
                    h256_to_u256(system_env.base_system_smart_contracts.default_aa.hash),
                );
                VmInstanceData::m5(
                    oracle_tools,
                    block_properties,
                    vm_m5::vm::MultiVMSubversion::V2,
                )
            }
            VmVersion::M6Initial => {
                let oracle_tools = OracleTools::new(vm_version, storage_view, history);
                let block_properties = BlockProperties::new(
                    vm_version,
                    h256_to_u256(system_env.base_system_smart_contracts.default_aa.hash),
                );
                VmInstanceData::m6(
                    oracle_tools,
                    block_properties,
                    vm_m6::vm::MultiVMSubversion::V1,
                )
            }
            VmVersion::M6BugWithCompressionFixed => {
                let oracle_tools = OracleTools::new(vm_version, storage_view, history);
                let block_properties = BlockProperties::new(
                    vm_version,
                    h256_to_u256(system_env.base_system_smart_contracts.default_aa.hash),
                );
                VmInstanceData::m6(
                    oracle_tools,
                    block_properties,
                    vm_m6::vm::MultiVMSubversion::V2,
                )
            }
            VmVersion::Vm1_3_2 => VmInstanceData::vm1_3_2(storage_view, history),
            VmVersion::VmVirtualBlocks => VmInstanceData::vm_virtual_blocks(storage_view, history),
        }
    }
}

impl<S: ReadStorage> VmInstance<'_, S, vm_virtual_blocks::HistoryEnabled> {
    pub fn make_snapshot(&mut self) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.save_current_vm_as_snapshot(),
            VmInstanceVersion::VmM6(vm) => vm.save_current_vm_as_snapshot(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.save_current_vm_as_snapshot(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.make_snapshot(),
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
        }
    }
}
