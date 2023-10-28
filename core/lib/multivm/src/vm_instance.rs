use crate::interface::{
    FinishedL1Batch, L2BlockEnv, VmExecutionMode, VmInterface, VmInterfaceHistoryEnabled,
    VmMemoryMetrics,
};

use zksync_state::{ReadStorage, StorageView};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::glue::history_mode::HistoryMode;
use crate::glue::tracers::MultivmTracer;
use crate::glue::GlueInto;

pub struct VmInstance<S: ReadStorage, H: HistoryMode> {
    pub(crate) vm: VmInstanceVersion<S, H>,
}

#[derive(Debug)]
pub(crate) enum VmInstanceVersion<S: ReadStorage, H: HistoryMode> {
    VmM5(crate::vm_m5::Vm<StorageView<S>, H>),
    VmM6(crate::vm_m6::Vm<StorageView<S>, H>),
    Vm1_3_2(crate::vm_1_3_2::Vm<StorageView<S>, H>),
    VmVirtualBlocks(crate::vm_virtual_blocks::Vm<StorageView<S>, H>),
    VmVirtualBlocksRefundsEnhancement(crate::vm_latest::Vm<StorageView<S>, H>),
}

impl<S: ReadStorage, H: HistoryMode> VmInstance<S, H> {
    /// Push tx into memory for the future execution
    pub fn push_transaction(&mut self, tx: &zksync_types::Transaction) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstanceVersion::VmM6(vm) => {
                vm.push_transaction(tx.clone());
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                vm.push_transaction(tx.clone());
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
            VmInstanceVersion::VmM5(vm) => vm.finish_batch(),
            VmInstanceVersion::VmM6(vm) => vm.finish_batch(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.finish_batch(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.finish_batch(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => vm.finish_batch(),
        }
    }

    /// Execute the batch without stops after each tx.
    /// This method allows to execute the part  of the VM cycle after executing all txs.
    pub fn execute_block_tip(&mut self) -> crate::interface::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.execute(VmExecutionMode::Bootloader),
            VmInstanceVersion::VmM6(vm) => vm.execute(VmExecutionMode::Bootloader),
            VmInstanceVersion::Vm1_3_2(vm) => vm.execute(VmExecutionMode::Bootloader),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.execute(VmExecutionMode::Bootloader),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.execute(VmExecutionMode::Bootloader)
            }
        }
    }

    /// Execute next transaction and stop vm right after next transaction execution
    pub fn execute_next_transaction(&mut self) -> crate::interface::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.execute(VmExecutionMode::OneTx),
            VmInstanceVersion::VmM6(vm) => vm.execute(VmExecutionMode::OneTx),
            VmInstanceVersion::Vm1_3_2(vm) => vm.execute(VmExecutionMode::OneTx),
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.execute(VmExecutionMode::OneTx).glue_into()
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
            VmInstanceVersion::Vm1_3_2(vm) => vm.get_last_tx_compressed_bytecodes(),
            VmInstanceVersion::VmM6(vm) => vm.get_last_tx_compressed_bytecodes(),
            VmInstanceVersion::VmM5(vm) => vm.get_last_tx_compressed_bytecodes(),
        }
    }

    /// Execute next transaction with custom tracers
    pub fn inspect_next_transaction(
        &mut self,
        tracers: Vec<Box<dyn MultivmTracer<StorageView<S>, H>>>,
    ) -> crate::interface::VmExecutionResultAndLogs {
        match &mut self.vm {
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                let tracers: Vec<_> = tracers.into_iter().map(|t| t.vm_virtual_blocks()).collect();
                vm.inspect(tracers.into(), VmExecutionMode::OneTx)
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                let tracers: Vec<_> = tracers.into_iter().map(|t| t.latest()).collect();
                vm.inspect(tracers.into(), VmExecutionMode::OneTx)
            }
            VmInstanceVersion::Vm1_3_2(vm) => vm.execute(VmExecutionMode::OneTx),
            VmInstanceVersion::VmM6(vm) => vm.execute(VmExecutionMode::OneTx),
            VmInstanceVersion::VmM5(vm) => vm.execute(VmExecutionMode::OneTx),
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
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstanceVersion::VmM6(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstanceVersion::VmVirtualBlocks(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                vm.execute_transaction_with_bytecode_compression(tx, with_compression)
            }
        }
    }

    /// Inspect transaction with optional bytecode compression.
    pub fn inspect_transaction_with_bytecode_compression(
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
                let tracers: Vec<_> = tracers.into_iter().map(|t| t.vm_virtual_blocks()).collect();
                vm.inspect_transaction_with_bytecode_compression(
                    tracers.into(),
                    tx,
                    with_compression,
                )
            }
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                let tracers: Vec<_> = tracers.into_iter().map(|t| t.latest()).collect();
                vm.inspect_transaction_with_bytecode_compression(
                    tracers.into(),
                    tx,
                    with_compression,
                )
            }
            VmInstanceVersion::VmM6(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
            VmInstanceVersion::Vm1_3_2(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
            VmInstanceVersion::VmM5(vm) => vm.inspect_transaction_with_bytecode_compression(
                Default::default(),
                tx,
                with_compression,
            ),
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
            VmInstanceVersion::VmM6(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            VmInstanceVersion::Vm1_3_2(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
            VmInstanceVersion::VmM5(vm) => {
                vm.start_new_l2_block(l2_block_env);
            }
        }
    }

    pub fn record_vm_memory_metrics(&self) -> Option<VmMemoryMetrics> {
        match &self.vm {
            VmInstanceVersion::VmM5(vm) => Some(vm.record_vm_memory_metrics()),
            VmInstanceVersion::VmM6(vm) => Some(vm.record_vm_memory_metrics()),
            VmInstanceVersion::Vm1_3_2(vm) => Some(vm.record_vm_memory_metrics()),
            VmInstanceVersion::VmVirtualBlocks(vm) => Some(vm.record_vm_memory_metrics()),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => {
                Some(vm.record_vm_memory_metrics())
            }
        }
    }
}

impl<S: ReadStorage> VmInstance<S, crate::vm_latest::HistoryEnabled> {
    pub fn make_snapshot(&mut self) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.make_snapshot(),
            VmInstanceVersion::VmM6(vm) => vm.make_snapshot(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.make_snapshot(),
            VmInstanceVersion::VmVirtualBlocks(vm) => vm.make_snapshot(),
            VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm) => vm.make_snapshot(),
        }
    }

    pub fn rollback_to_the_latest_snapshot(&mut self) {
        match &mut self.vm {
            VmInstanceVersion::VmM5(vm) => vm.rollback_to_the_latest_snapshot(),
            VmInstanceVersion::VmM6(vm) => vm.rollback_to_the_latest_snapshot(),
            VmInstanceVersion::Vm1_3_2(vm) => vm.rollback_to_the_latest_snapshot(),
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
                vm.pop_snapshot_no_rollback();
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
