use crate::interface::{
    BootloaderMemory, CurrentExecutionState, FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv,
    VmExecutionMode, VmExecutionResultAndLogs, VmInterface, VmInterfaceHistoryEnabled,
    VmMemoryMetrics,
};

use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::VmVersion;
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::glue::history_mode::HistoryMode;
use crate::tracers::TracerDispatcher;

#[derive(Debug)]
pub enum VmInstance<S: WriteStorage, H: HistoryMode> {
    VmM5(crate::vm_m5::Vm<S, H>),
    VmM6(crate::vm_m6::Vm<S, H>),
    Vm1_3_2(crate::vm_1_3_2::Vm<S, H>),
    VmVirtualBlocks(crate::vm_virtual_blocks::Vm<S, H>),
    VmVirtualBlocksRefundsEnhancement(crate::vm_latest::Vm<S, H>),
}

impl<S: WriteStorage, H: HistoryMode> VmInterface<S, H> for VmInstance<S, H> {
    type TracerDispatcher = TracerDispatcher<S, H>;
    /// Push tx into memory for the future execution
    fn push_transaction(&mut self, tx: zksync_types::Transaction) {
        match self {
            VmInstance::VmM5(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstance::VmM6(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstance::Vm1_3_2(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstance::VmVirtualBlocks(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.push_transaction(tx.clone());
            }
        }
    }

    /// Return the results of execution of all batch
    fn finish_batch(&mut self) -> FinishedL1Batch {
        match self {
            VmInstance::VmM5(vm) => vm.finish_batch(),
            VmInstance::VmM6(vm) => vm.finish_batch(),
            VmInstance::Vm1_3_2(vm) => vm.finish_batch(),
            VmInstance::VmVirtualBlocks(vm) => vm.finish_batch(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.finish_batch(),
        }
    }

    /// Execute the batch without stops after each tx.
    /// This method allows to execute the part  of the VM cycle after executing all txs.
    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        match self {
            VmInstance::VmM5(vm) => vm.execute(execution_mode),
            VmInstance::VmM6(vm) => vm.execute(execution_mode),
            VmInstance::Vm1_3_2(vm) => vm.execute(execution_mode),
            VmInstance::VmVirtualBlocks(vm) => vm.execute(execution_mode),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.execute(execution_mode),
        }
    }

    /// Get compressed bytecodes of the last executed transaction
    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        match self {
            VmInstance::VmVirtualBlocks(vm) => vm.get_last_tx_compressed_bytecodes(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.get_last_tx_compressed_bytecodes()
            }
            VmInstance::Vm1_3_2(vm) => vm.get_last_tx_compressed_bytecodes(),
            VmInstance::VmM6(vm) => vm.get_last_tx_compressed_bytecodes(),
            VmInstance::VmM5(vm) => vm.get_last_tx_compressed_bytecodes(),
        }
    }

    /// Execute next transaction with custom tracers
    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        match self {
            VmInstance::VmVirtualBlocks(vm) => {
                vm.inspect(dispatcher.into(), VmExecutionMode::OneTx)
            }
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.inspect(dispatcher.into(), VmExecutionMode::OneTx)
            }
            VmInstance::Vm1_3_2(vm) => vm.execute(execution_mode),
            VmInstance::VmM6(vm) => vm.execute(execution_mode),
            VmInstance::VmM5(vm) => vm.execute(execution_mode),
        }
    }

    /// Execute transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> Result<VmExecutionResultAndLogs, crate::interface::BytecodeCompressionError> {
        match self {
            VmInstance::VmM5(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstance::VmM6(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstance::Vm1_3_2(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstance::VmVirtualBlocks(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
        }
    }

    /// Inspect transaction with optional bytecode compression.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> Result<
        crate::interface::VmExecutionResultAndLogs,
        crate::interface::BytecodeCompressionError,
    > {
        match self {
            VmInstance::VmVirtualBlocks(vm) => vm.inspect_transaction_with_bytecode_compression(
                dispatcher.into(),
                tx,
                with_compression,
            ),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm
                .inspect_transaction_with_bytecode_compression(
                    dispatcher.into(),
                    tx,
                    with_compression,
                ),
            VmInstance::VmM6(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
            VmInstance::Vm1_3_2(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
            VmInstance::VmM5(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
        }
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        match self {
            VmInstance::VmVirtualBlocks(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            VmInstance::VmM6(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            VmInstance::Vm1_3_2(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            VmInstance::VmM5(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
        }
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        match &self {
            VmInstance::VmM5(vm) => vm.record_vm_memory_metrics(),
            VmInstance::VmM6(vm) => vm.record_vm_memory_metrics(),
            VmInstance::Vm1_3_2(vm) => vm.record_vm_memory_metrics(),
            VmInstance::VmVirtualBlocks(vm) => vm.record_vm_memory_metrics(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.record_vm_memory_metrics(),
        }
    }

    fn new(batch_env: L1BatchEnv, system_env: SystemEnv, storage_view: StoragePtr<S>) -> Self {
        let protocol_version = system_env.version;
        let vm_version: VmVersion = protocol_version.into();
        Self::new_with_specific_version(batch_env, system_env, storage_view, vm_version)
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        match &self {
            VmInstance::VmM5(vm) => vm.get_bootloader_memory(),
            VmInstance::VmM6(vm) => vm.get_bootloader_memory(),
            VmInstance::Vm1_3_2(vm) => vm.get_bootloader_memory(),
            VmInstance::VmVirtualBlocks(vm) => vm.get_bootloader_memory(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.get_bootloader_memory(),
        }
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        match &self {
            VmInstance::VmM5(vm) => vm.get_current_execution_state(),
            VmInstance::VmM6(vm) => vm.get_current_execution_state(),
            VmInstance::Vm1_3_2(vm) => vm.get_current_execution_state(),
            VmInstance::VmVirtualBlocks(vm) => vm.get_current_execution_state(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.get_current_execution_state(),
        }
    }
}

impl<S: WriteStorage> VmInterfaceHistoryEnabled<S>
    for VmInstance<S, crate::vm_latest::HistoryEnabled>
{
    fn make_snapshot(&mut self) {
        match self {
            VmInstance::VmM5(vm) => vm.make_snapshot(),
            VmInstance::VmM6(vm) => vm.make_snapshot(),
            VmInstance::Vm1_3_2(vm) => vm.make_snapshot(),
            VmInstance::VmVirtualBlocks(vm) => vm.make_snapshot(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.make_snapshot(),
        }
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        match self {
            VmInstance::VmM5(vm) => vm.rollback_to_the_latest_snapshot(),
            VmInstance::VmM6(vm) => vm.rollback_to_the_latest_snapshot(),
            VmInstance::Vm1_3_2(vm) => vm.rollback_to_the_latest_snapshot(),
            VmInstance::VmVirtualBlocks(vm) => {
                vm.rollback_to_the_latest_snapshot();
            }
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.rollback_to_the_latest_snapshot();
            }
        }
    }

    fn pop_snapshot_no_rollback(&mut self) {
        match self {
            VmInstance::VmM5(vm) => {
                // A dedicated method was added later.
                vm.pop_snapshot_no_rollback();
            }
            VmInstance::VmM6(vm) => vm.pop_snapshot_no_rollback(),
            VmInstance::Vm1_3_2(vm) => vm.pop_snapshot_no_rollback(),
            VmInstance::VmVirtualBlocks(vm) => vm.pop_snapshot_no_rollback(),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.pop_snapshot_no_rollback(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmInstance<S, H> {
    pub fn new_with_specific_version(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<S>,
        vm_version: VmVersion,
    ) -> Self {
        match vm_version {
            VmVersion::M5WithoutRefunds => {
                let vm = crate::vm_m5::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                    crate::vm_m5::vm_instance::MultiVMSubversion::V1,
                );
                VmInstance::VmM5(vm)
            }
            VmVersion::M5WithRefunds => {
                let vm = crate::vm_m5::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                    crate::vm_m5::vm_instance::MultiVMSubversion::V2,
                );
                VmInstance::VmM5(vm)
            }
            VmVersion::M6Initial => {
                let vm = crate::vm_m6::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                    crate::vm_m6::vm_instance::MultiVMSubversion::V1,
                );
                VmInstance::VmM6(vm)
            }
            VmVersion::M6BugWithCompressionFixed => {
                let vm = crate::vm_m6::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                    crate::vm_m6::vm_instance::MultiVMSubversion::V2,
                );
                VmInstance::VmM6(vm)
            }
            VmVersion::Vm1_3_2 => {
                let vm = crate::vm_1_3_2::Vm::new(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                );
                VmInstance::Vm1_3_2(vm)
            }
            VmVersion::VmVirtualBlocks => {
                let vm = crate::vm_virtual_blocks::Vm::new(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                );
                VmInstance::VmVirtualBlocks(vm)
            }
            VmVersion::VmVirtualBlocksRefundsEnhancement => {
                let vm = crate::vm_latest::Vm::new(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                );
                VmInstance::VmVirtualBlocksRefundsEnhancement(vm)
            }
        }
    }
}
