use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::glue::GlueInto;

#[derive(Debug)]
pub enum VmInstance<'a> {
    VmM5(Box<vm_m5::VmInstance<'a>>),
    VmM6(Box<vm_m6::VmInstance<'a, vm_m6::HistoryEnabled>>),
    Vm1_3_2(Box<vm_vm1_3_2::VmInstance<'a, vm_vm1_3_2::HistoryEnabled>>),
}

impl<'a> VmInstance<'a> {
    pub fn gas_consumed(&self) -> u32 {
        match self {
            VmInstance::VmM5(vm) => vm.gas_consumed(),
            VmInstance::VmM6(vm) => vm.gas_consumed(),
            VmInstance::Vm1_3_2(vm) => vm.gas_consumed(),
        }
    }

    pub fn save_current_vm_as_snapshot(&mut self) {
        match self {
            VmInstance::VmM5(vm) => vm.save_current_vm_as_snapshot(),
            VmInstance::VmM6(vm) => vm.save_current_vm_as_snapshot(),
            VmInstance::Vm1_3_2(vm) => vm.save_current_vm_as_snapshot(),
        }
    }

    pub fn rollback_to_snapshot_popping(&mut self) {
        match self {
            VmInstance::VmM5(vm) => vm.rollback_to_latest_snapshot_popping(),
            VmInstance::VmM6(vm) => vm.rollback_to_latest_snapshot_popping(),
            VmInstance::Vm1_3_2(vm) => vm.rollback_to_latest_snapshot_popping(),
        }
    }

    pub fn pop_snapshot_no_rollback(&mut self) {
        match self {
            VmInstance::VmM5(vm) => {
                // A dedicated method was added later.
                vm.snapshots.pop().unwrap();
            }
            VmInstance::VmM6(vm) => vm.pop_snapshot_no_rollback(),
            VmInstance::Vm1_3_2(vm) => vm.pop_snapshot_no_rollback(),
        }
    }

    pub fn execute_till_block_end(
        &mut self,
        job_type: vm_vm1_3_2::vm_with_bootloader::BootloaderJobType,
    ) -> vm_vm1_3_2::VmBlockResult {
        match self {
            VmInstance::VmM5(vm) => vm.execute_till_block_end(job_type.glue_into()).glue_into(),
            VmInstance::VmM6(vm) => vm.execute_till_block_end(job_type.glue_into()).glue_into(),
            VmInstance::Vm1_3_2(vm) => vm.execute_till_block_end(job_type).glue_into(),
        }
    }

    pub fn is_bytecode_known(&self, bytecode_hash: &zksync_types::H256) -> bool {
        match self {
            VmInstance::VmM5(_) => {
                false
            }
            VmInstance::VmM6(vm) => vm
                .state
                .storage
                .storage
                .get_ptr()
                .borrow_mut()
                .is_bytecode_known(bytecode_hash),
            VmInstance::Vm1_3_2(vm) => vm
                .state
                .storage
                .storage
                .get_ptr()
                .borrow_mut()
                .is_bytecode_known(bytecode_hash),
        }
    }

    pub fn push_transaction_to_bootloader_memory(
        &mut self,
        tx: &zksync_types::Transaction,
        execution_mode: vm_vm1_3_2::vm_with_bootloader::TxExecutionMode,
        explicit_compressed_bytecodes: Option<Vec<CompressedBytecodeInfo>>,
    ) {
        match self {
            VmInstance::VmM5(vm) => {
                vm_m5::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    execution_mode.glue_into(),
                )
            }
            VmInstance::VmM6(vm) => {
                vm_m6::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    execution_mode.glue_into(),
                    explicit_compressed_bytecodes,
                )
            }
            VmInstance::Vm1_3_2(vm) => {
                vm_vm1_3_2::vm_with_bootloader::push_transaction_to_bootloader_memory(
                    vm,
                    tx,
                    execution_mode,
                    explicit_compressed_bytecodes,
                )
            }
        }
    }

    pub fn execute_next_tx(
        &mut self,
        validation_computational_gas_limit: u32,
        with_call_tracer: bool,
    ) -> Result<vm_vm1_3_2::vm::VmTxExecutionResult, vm_vm1_3_2::TxRevertReason> {
        match self {
            VmInstance::VmM5(vm) => vm
                .execute_next_tx()
                .map(GlueInto::glue_into)
                .map_err(GlueInto::glue_into),
            VmInstance::VmM6(vm) => vm
                .execute_next_tx(validation_computational_gas_limit, with_call_tracer)
                .map(GlueInto::glue_into)
                .map_err(GlueInto::glue_into),
            VmInstance::Vm1_3_2(vm) => {
                vm.execute_next_tx(validation_computational_gas_limit, with_call_tracer)
            }
        }
    }

    pub fn execute_block_tip(&mut self) -> vm_vm1_3_2::vm::VmPartialExecutionResult {
        match self {
            VmInstance::VmM5(vm) => vm.execute_block_tip().glue_into(),
            VmInstance::VmM6(vm) => vm.execute_block_tip().glue_into(),
            VmInstance::Vm1_3_2(vm) => vm.execute_block_tip(),
        }
    }
}
