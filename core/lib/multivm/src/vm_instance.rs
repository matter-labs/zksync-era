use zksync_types::vm::{FastVmMode, VmVersion};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::{
    glue::history_mode::HistoryMode,
    interface::{
        storage::{ImmutableStorageView, ReadStorage, StoragePtr, StorageView},
        BootloaderMemory, BytecodeCompressionError, CurrentExecutionState, FinishedL1Batch,
        L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode, VmExecutionResultAndLogs, VmFactory,
        VmInterface, VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    tracers::TracerDispatcher,
    versions::shadow::ShadowVm,
};

pub type ShadowedFastVm<S, H> = ShadowVm<S, crate::vm_latest::Vm<StorageView<S>, H>>;

#[derive(Debug)]
pub enum VmInstance<S: ReadStorage, H: HistoryMode> {
    VmM5(crate::vm_m5::Vm<StorageView<S>, H>),
    VmM6(crate::vm_m6::Vm<StorageView<S>, H>),
    Vm1_3_2(crate::vm_1_3_2::Vm<StorageView<S>, H>),
    VmVirtualBlocks(crate::vm_virtual_blocks::Vm<StorageView<S>, H>),
    VmVirtualBlocksRefundsEnhancement(crate::vm_refunds_enhancement::Vm<StorageView<S>, H>),
    VmBoojumIntegration(crate::vm_boojum_integration::Vm<StorageView<S>, H>),
    Vm1_4_1(crate::vm_1_4_1::Vm<StorageView<S>, H>),
    Vm1_4_2(crate::vm_1_4_2::Vm<StorageView<S>, H>),
    Vm1_5_0(crate::vm_latest::Vm<StorageView<S>, H>),
    VmFast(crate::vm_fast::Vm<ImmutableStorageView<S>>),
    ShadowedVmFast(ShadowedFastVm<S, H>),
}

macro_rules! dispatch_vm {
    ($self:ident.$function:ident($($params:tt)*)) => {
        match $self {
            VmInstance::VmM5(vm) => vm.$function($($params)*),
            VmInstance::VmM6(vm) => vm.$function($($params)*),
            VmInstance::Vm1_3_2(vm) => vm.$function($($params)*),
            VmInstance::VmVirtualBlocks(vm) => vm.$function($($params)*),
            VmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.$function($($params)*),
            VmInstance::VmBoojumIntegration(vm) => vm.$function($($params)*),
            VmInstance::Vm1_4_1(vm) => vm.$function($($params)*),
            VmInstance::Vm1_4_2(vm) => vm.$function($($params)*),
            VmInstance::Vm1_5_0(vm) => vm.$function($($params)*),
            VmInstance::VmFast(vm) => vm.$function($($params)*),
            VmInstance::ShadowedVmFast(vm) => vm.$function($($params)*),
        }
    };
}

impl<S: ReadStorage, H: HistoryMode> VmInterface for VmInstance<S, H> {
    type TracerDispatcher = TracerDispatcher<StorageView<S>, H>;

    /// Push tx into memory for the future execution
    fn push_transaction(&mut self, tx: zksync_types::Transaction) {
        dispatch_vm!(self.push_transaction(tx))
    }

    /// Execute the batch without stops after each tx.
    /// This method allows to execute the part  of the VM cycle after executing all txs.
    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        dispatch_vm!(self.execute(execution_mode))
    }

    /// Execute next transaction with custom tracers
    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        dispatch_vm!(self.inspect(dispatcher.into(), execution_mode))
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        dispatch_vm!(self.get_bootloader_memory())
    }

    /// Get compressed bytecodes of the last executed transaction
    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        dispatch_vm!(self.get_last_tx_compressed_bytecodes())
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        dispatch_vm!(self.start_new_l2_block(l2_block_env))
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        dispatch_vm!(self.get_current_execution_state())
    }

    /// Execute transaction with optional bytecode compression.
    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        dispatch_vm!(self.execute_transaction_with_bytecode_compression(tx, with_compression))
    }

    /// Inspect transaction with optional bytecode compression.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        tx: zksync_types::Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        dispatch_vm!(self.inspect_transaction_with_bytecode_compression(
            dispatcher.into(),
            tx,
            with_compression
        ))
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        dispatch_vm!(self.record_vm_memory_metrics())
    }

    fn gas_remaining(&self) -> u32 {
        dispatch_vm!(self.gas_remaining())
    }

    /// Return the results of execution of all batch
    fn finish_batch(&mut self) -> FinishedL1Batch {
        dispatch_vm!(self.finish_batch())
    }
}

impl<S: ReadStorage, H: HistoryMode> VmFactory<StorageView<S>> for VmInstance<S, H> {
    fn new(
        batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
    ) -> Self {
        let protocol_version = system_env.version;
        let vm_version: VmVersion = protocol_version.into();
        Self::new_with_specific_version(batch_env, system_env, storage_view, vm_version)
    }
}

impl<S: ReadStorage> VmInterfaceHistoryEnabled for VmInstance<S, crate::vm_latest::HistoryEnabled> {
    fn make_snapshot(&mut self) {
        dispatch_vm!(self.make_snapshot())
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        dispatch_vm!(self.rollback_to_the_latest_snapshot())
    }

    fn pop_snapshot_no_rollback(&mut self) {
        dispatch_vm!(self.pop_snapshot_no_rollback())
    }
}

impl<S: ReadStorage, H: HistoryMode> VmInstance<S, H> {
    pub fn new_with_specific_version(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
        vm_version: VmVersion,
    ) -> Self {
        match vm_version {
            VmVersion::M5WithoutRefunds => {
                let vm = crate::vm_m5::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m5::vm_instance::MultiVMSubversion::V1,
                );
                VmInstance::VmM5(vm)
            }
            VmVersion::M5WithRefunds => {
                let vm = crate::vm_m5::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m5::vm_instance::MultiVMSubversion::V2,
                );
                VmInstance::VmM5(vm)
            }
            VmVersion::M6Initial => {
                let vm = crate::vm_m6::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m6::vm_instance::MultiVMSubversion::V1,
                );
                VmInstance::VmM6(vm)
            }
            VmVersion::M6BugWithCompressionFixed => {
                let vm = crate::vm_m6::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m6::vm_instance::MultiVMSubversion::V2,
                );
                VmInstance::VmM6(vm)
            }
            VmVersion::Vm1_3_2 => {
                let vm = crate::vm_1_3_2::Vm::new(l1_batch_env, system_env, storage_view);
                VmInstance::Vm1_3_2(vm)
            }
            VmVersion::VmVirtualBlocks => {
                let vm = crate::vm_virtual_blocks::Vm::new(l1_batch_env, system_env, storage_view);
                VmInstance::VmVirtualBlocks(vm)
            }
            VmVersion::VmVirtualBlocksRefundsEnhancement => {
                let vm =
                    crate::vm_refunds_enhancement::Vm::new(l1_batch_env, system_env, storage_view);
                VmInstance::VmVirtualBlocksRefundsEnhancement(vm)
            }
            VmVersion::VmBoojumIntegration => {
                let vm =
                    crate::vm_boojum_integration::Vm::new(l1_batch_env, system_env, storage_view);
                VmInstance::VmBoojumIntegration(vm)
            }
            VmVersion::Vm1_4_1 => {
                let vm = crate::vm_1_4_1::Vm::new(l1_batch_env, system_env, storage_view);
                VmInstance::Vm1_4_1(vm)
            }
            VmVersion::Vm1_4_2 => {
                let vm = crate::vm_1_4_2::Vm::new(l1_batch_env, system_env, storage_view);
                VmInstance::Vm1_4_2(vm)
            }
            VmVersion::Vm1_5_0SmallBootloaderMemory => {
                let vm = crate::vm_latest::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_latest::MultiVMSubversion::SmallBootloaderMemory,
                );
                VmInstance::Vm1_5_0(vm)
            }
            VmVersion::Vm1_5_0IncreasedBootloaderMemory => {
                let vm = crate::vm_latest::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_latest::MultiVMSubversion::IncreasedBootloaderMemory,
                );
                VmInstance::Vm1_5_0(vm)
            }
        }
    }

    /// Creates a VM that may use the fast VM depending on the protocol version in `system_env` and `mode`.
    pub fn maybe_fast(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
        mode: FastVmMode,
    ) -> Self {
        let vm_version = system_env.version.into();
        match vm_version {
            VmVersion::Vm1_5_0IncreasedBootloaderMemory => match mode {
                FastVmMode::Old => Self::new(l1_batch_env, system_env, storage_view),
                FastVmMode::New => {
                    let storage = ImmutableStorageView::new(storage_view);
                    Self::VmFast(crate::vm_fast::Vm::new(l1_batch_env, system_env, storage))
                }
                FastVmMode::Shadow => {
                    Self::ShadowedVmFast(ShadowVm::new(l1_batch_env, system_env, storage_view))
                }
            },
            _ => Self::new(l1_batch_env, system_env, storage_view),
        }
    }
}
