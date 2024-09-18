use zksync_types::{vm::VmVersion, Transaction};

use crate::{
    glue::history_mode::HistoryMode,
    interface::{
        storage::{ImmutableStorageView, ReadStorage, StoragePtr, StorageView},
        BytecodeCompressionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv,
        VmExecutionMode, VmExecutionResultAndLogs, VmFactory, VmInterface,
        VmInterfaceHistoryEnabled, VmMemoryMetrics,
    },
    tracers::TracerDispatcher,
    versions::shadow::ShadowVm,
    vm_latest::HistoryEnabled,
};

#[derive(Debug)]
pub enum LegacyVmInstance<S: ReadStorage, H: HistoryMode> {
    VmM5(crate::vm_m5::Vm<StorageView<S>, H>),
    VmM6(crate::vm_m6::Vm<StorageView<S>, H>),
    Vm1_3_2(crate::vm_1_3_2::Vm<StorageView<S>, H>),
    VmVirtualBlocks(crate::vm_virtual_blocks::Vm<StorageView<S>, H>),
    VmVirtualBlocksRefundsEnhancement(crate::vm_refunds_enhancement::Vm<StorageView<S>, H>),
    VmBoojumIntegration(crate::vm_boojum_integration::Vm<StorageView<S>, H>),
    Vm1_4_1(crate::vm_1_4_1::Vm<StorageView<S>, H>),
    Vm1_4_2(crate::vm_1_4_2::Vm<StorageView<S>, H>),
    Vm1_5_0(crate::vm_latest::Vm<StorageView<S>, H>),
}

macro_rules! dispatch_legacy_vm {
    ($self:ident.$function:ident($($params:tt)*)) => {
        match $self {
            LegacyVmInstance::VmM5(vm) => vm.$function($($params)*),
            LegacyVmInstance::VmM6(vm) => vm.$function($($params)*),
            LegacyVmInstance::Vm1_3_2(vm) => vm.$function($($params)*),
            LegacyVmInstance::VmVirtualBlocks(vm) => vm.$function($($params)*),
            LegacyVmInstance::VmVirtualBlocksRefundsEnhancement(vm) => vm.$function($($params)*),
            LegacyVmInstance::VmBoojumIntegration(vm) => vm.$function($($params)*),
            LegacyVmInstance::Vm1_4_1(vm) => vm.$function($($params)*),
            LegacyVmInstance::Vm1_4_2(vm) => vm.$function($($params)*),
            LegacyVmInstance::Vm1_5_0(vm) => vm.$function($($params)*),
        }
    };
}

impl<S: ReadStorage, H: HistoryMode> VmInterface for LegacyVmInstance<S, H> {
    type TracerDispatcher = TracerDispatcher<StorageView<S>, H>;

    /// Push tx into memory for the future execution
    fn push_transaction(&mut self, tx: Transaction) {
        dispatch_legacy_vm!(self.push_transaction(tx))
    }

    /// Execute next transaction with custom tracers
    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        dispatch_legacy_vm!(self.inspect(dispatcher.into(), execution_mode))
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        dispatch_legacy_vm!(self.start_new_l2_block(l2_block_env))
    }

    /// Inspect transaction with optional bytecode compression.
    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        dispatch_legacy_vm!(self.inspect_transaction_with_bytecode_compression(
            dispatcher.into(),
            tx,
            with_compression
        ))
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        dispatch_legacy_vm!(self.record_vm_memory_metrics())
    }

    /// Return the results of execution of all batch
    fn finish_batch(&mut self) -> FinishedL1Batch {
        dispatch_legacy_vm!(self.finish_batch())
    }
}

impl<S: ReadStorage, H: HistoryMode> VmFactory<StorageView<S>> for LegacyVmInstance<S, H> {
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

impl<S: ReadStorage> VmInterfaceHistoryEnabled for LegacyVmInstance<S, HistoryEnabled> {
    fn make_snapshot(&mut self) {
        dispatch_legacy_vm!(self.make_snapshot());
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        dispatch_legacy_vm!(self.rollback_to_the_latest_snapshot());
    }

    fn pop_snapshot_no_rollback(&mut self) {
        dispatch_legacy_vm!(self.pop_snapshot_no_rollback());
    }
}

impl<S: ReadStorage, H: HistoryMode> LegacyVmInstance<S, H> {
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
                LegacyVmInstance::VmM5(vm)
            }
            VmVersion::M5WithRefunds => {
                let vm = crate::vm_m5::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m5::vm_instance::MultiVMSubversion::V2,
                );
                LegacyVmInstance::VmM5(vm)
            }
            VmVersion::M6Initial => {
                let vm = crate::vm_m6::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m6::vm_instance::MultiVMSubversion::V1,
                );
                LegacyVmInstance::VmM6(vm)
            }
            VmVersion::M6BugWithCompressionFixed => {
                let vm = crate::vm_m6::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_m6::vm_instance::MultiVMSubversion::V2,
                );
                LegacyVmInstance::VmM6(vm)
            }
            VmVersion::Vm1_3_2 => {
                let vm = crate::vm_1_3_2::Vm::new(l1_batch_env, system_env, storage_view);
                LegacyVmInstance::Vm1_3_2(vm)
            }
            VmVersion::VmVirtualBlocks => {
                let vm = crate::vm_virtual_blocks::Vm::new(l1_batch_env, system_env, storage_view);
                LegacyVmInstance::VmVirtualBlocks(vm)
            }
            VmVersion::VmVirtualBlocksRefundsEnhancement => {
                let vm =
                    crate::vm_refunds_enhancement::Vm::new(l1_batch_env, system_env, storage_view);
                LegacyVmInstance::VmVirtualBlocksRefundsEnhancement(vm)
            }
            VmVersion::VmBoojumIntegration => {
                let vm =
                    crate::vm_boojum_integration::Vm::new(l1_batch_env, system_env, storage_view);
                LegacyVmInstance::VmBoojumIntegration(vm)
            }
            VmVersion::Vm1_4_1 => {
                let vm = crate::vm_1_4_1::Vm::new(l1_batch_env, system_env, storage_view);
                LegacyVmInstance::Vm1_4_1(vm)
            }
            VmVersion::Vm1_4_2 => {
                let vm = crate::vm_1_4_2::Vm::new(l1_batch_env, system_env, storage_view);
                LegacyVmInstance::Vm1_4_2(vm)
            }
            VmVersion::Vm1_5_0SmallBootloaderMemory => {
                let vm = crate::vm_latest::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_latest::MultiVMSubversion::SmallBootloaderMemory,
                );
                LegacyVmInstance::Vm1_5_0(vm)
            }
            VmVersion::Vm1_5_0IncreasedBootloaderMemory => {
                let vm = crate::vm_latest::Vm::new_with_subversion(
                    l1_batch_env,
                    system_env,
                    storage_view,
                    crate::vm_latest::MultiVMSubversion::IncreasedBootloaderMemory,
                );
                LegacyVmInstance::Vm1_5_0(vm)
            }
        }
    }
}

/// Shadowed fast VM.
pub type ShadowedFastVm<S> = ShadowVm<S, crate::vm_latest::Vm<StorageView<S>, HistoryEnabled>>;

/// Fast VM variants.
// FIXME: generalize by tracer
#[derive(Debug)]
pub enum FastVmInstance<S: ReadStorage> {
    Fast(crate::vm_fast::Vm<ImmutableStorageView<S>>),
    Shadowed(ShadowedFastVm<S>),
}

macro_rules! dispatch_fast_vm {
    ($self:ident.$function:ident($($params:tt)*)) => {
        match $self {
            FastVmInstance::Fast(vm) => vm.$function($($params)*),
            FastVmInstance::Shadowed(vm) => vm.$function($($params)*),
        }
    };
}

impl<S: ReadStorage> VmInterface for FastVmInstance<S> {
    type TracerDispatcher = ();

    fn push_transaction(&mut self, tx: Transaction) {
        dispatch_fast_vm!(self.push_transaction(tx));
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        match self {
            Self::Fast(vm) => vm.inspect(dispatcher, execution_mode),
            Self::Shadowed(vm) => vm.inspect(Default::default(), execution_mode), // FIXME
        }
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        dispatch_fast_vm!(self.start_new_l2_block(l2_block_env));
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult<'_>, VmExecutionResultAndLogs) {
        match self {
            Self::Fast(vm) => {
                vm.inspect_transaction_with_bytecode_compression(tracer, tx, with_compression)
            }
            Self::Shadowed(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
        }
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        dispatch_fast_vm!(self.record_vm_memory_metrics())
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        dispatch_fast_vm!(self.finish_batch())
    }
}

impl<S: ReadStorage> VmInterfaceHistoryEnabled for FastVmInstance<S> {
    fn make_snapshot(&mut self) {
        dispatch_fast_vm!(self.make_snapshot());
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        dispatch_fast_vm!(self.rollback_to_the_latest_snapshot());
    }

    fn pop_snapshot_no_rollback(&mut self) {
        dispatch_fast_vm!(self.pop_snapshot_no_rollback());
    }
}

impl<S: ReadStorage> FastVmInstance<S> {
    pub fn fast(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
    ) -> Self {
        let storage = ImmutableStorageView::new(storage_view);
        Self::Fast(crate::vm_fast::Vm::new(l1_batch_env, system_env, storage))
    }

    pub fn shadowed(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
    ) -> Self {
        Self::Shadowed(ShadowedFastVm::new(l1_batch_env, system_env, storage_view))
    }
}
