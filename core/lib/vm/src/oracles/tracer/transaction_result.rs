use zk_evm::{
    abstractions::{
        AfterDecodingData, AfterExecutionData, BeforeExecutionData, Tracer, VmLocalStateData,
    },
    witness_trace::VmWitnessTracer,
    zkevm_opcode_defs::decoding::VmEncodingMode,
    zkevm_opcode_defs::FatPointer,
};
use zksync_types::U256;

use crate::memory::SimpleMemory;
use crate::oracles::tracer::utils::{print_debug_if_needed, read_pointer, VmHook};
use crate::oracles::tracer::{ExecutionEndTracer, PendingRefundTracer, PubdataSpentTracer};
use crate::vm::get_vm_hook_params;

#[derive(Debug, Clone, Default)]
pub(crate) struct TransactionResultTracer {
    pub(crate) revert_reason: Option<Vec<u8>>,
}

impl<const N: usize, E: VmEncodingMode<N>> VmWitnessTracer<N, E> for TransactionResultTracer {}

impl Tracer for TransactionResultTracer {
    type SupportedMemory = SimpleMemory;
    const CALL_BEFORE_EXECUTION: bool = true;

    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: &Self::SupportedMemory) {}
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &Self::SupportedMemory,
    ) {
    }
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &Self::SupportedMemory,
    ) {
        let hook = VmHook::from_opcode_memory(&state, &data);
        print_debug_if_needed(&hook, &state, memory);

        if matches!(hook, VmHook::ExecutionResult) {
            let vm_hook_params = get_vm_hook_params(memory);

            let success = vm_hook_params[0];
            let returndata_ptr = FatPointer::from_u256(vm_hook_params[1]);
            let returndata = read_pointer(memory, returndata_ptr);

            if success == U256::zero() {
                self.revert_reason = Some(returndata);
            } else {
                self.revert_reason = None;
            }
        }
    }
    fn after_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &Self::SupportedMemory,
    ) {
    }
}

impl ExecutionEndTracer for TransactionResultTracer {
    fn should_stop_execution(&self) -> bool {
        // This tracer will not prevent the execution from going forward
        // until the end of the block.
        false
    }
}

impl PendingRefundTracer for TransactionResultTracer {}
impl PubdataSpentTracer for TransactionResultTracer {}
