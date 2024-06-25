use zk_evm_1_5_0::{
    aux_structures::MemoryPage,
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{FarCallABI, FarCallForwardPageType, FatPointer, Opcode, UMAOpcode},
};
use zksync_types::U256;
use zksync_utils::u256_to_h256;

use crate::vm_latest::{
    constants::{
        get_vm_hook_params_start_position, get_vm_hook_position, BOOTLOADER_HEAP_PAGE,
        VM_HOOK_PARAMS_COUNT,
    },
    old_vm::{
        history_recorder::HistoryMode,
        memory::SimpleMemory,
        utils::{aux_heap_page_from_base, heap_page_from_base},
    },
    vm::MultiVMSubversion,
};

#[derive(Clone, Debug, Copy)]
pub(crate) enum VmHook {
    AccountValidationEntered,
    PaymasterValidationEntered,
    NoValidationEntered,
    ValidationStepEndeded,
    TxHasEnded,
    DebugLog,
    DebugReturnData,
    NoHook,
    NearCallCatch,
    AskOperatorForRefund,
    NotifyAboutRefund,
    ExecutionResult,
    FinalBatchInfo,
    // Hook used to signal that the final pubdata for a batch is requested.
    PubdataRequested,
}

impl VmHook {
    pub(crate) fn from_opcode_memory(
        state: &VmLocalStateData<'_>,
        data: &BeforeExecutionData,
        subversion: MultiVMSubversion,
    ) -> Self {
        let opcode_variant = data.opcode.variant;
        let heap_page =
            heap_page_from_base(state.vm_local_state.callstack.current.base_memory_page).0;

        let src0_value = data.src0_value.value;

        let fat_ptr = FatPointer::from_u256(src0_value);

        let value = data.src1_value.value;

        // Only `UMA` opcodes in the bootloader serve for vm hooks
        if !matches!(opcode_variant.opcode, Opcode::UMA(UMAOpcode::HeapWrite))
            || heap_page != BOOTLOADER_HEAP_PAGE
            || fat_ptr.offset != get_vm_hook_position(subversion) * 32
        {
            return Self::NoHook;
        }

        match value.as_u32() {
            0 => Self::AccountValidationEntered,
            1 => Self::PaymasterValidationEntered,
            2 => Self::NoValidationEntered,
            3 => Self::ValidationStepEndeded,
            4 => Self::TxHasEnded,
            5 => Self::DebugLog,
            6 => Self::DebugReturnData,
            7 => Self::NearCallCatch,
            8 => Self::AskOperatorForRefund,
            9 => Self::NotifyAboutRefund,
            10 => Self::ExecutionResult,
            11 => Self::FinalBatchInfo,
            12 => Self::PubdataRequested,
            _ => panic!("Unknown hook: {}", value.as_u32()),
        }
    }
}

pub(crate) fn get_debug_log<H: HistoryMode>(
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
    subversion: MultiVMSubversion,
) -> String {
    let vm_hook_params: Vec<_> = get_vm_hook_params(memory, subversion)
        .into_iter()
        .map(u256_to_h256)
        .collect();
    let mut msg = vm_hook_params[0].as_bytes().to_vec();
    while msg.last() == Some(&0) {
        msg.pop();
    }
    let data = vm_hook_params[1].as_bytes().to_vec();

    let msg = String::from_utf8(msg).expect("Invalid debug message");
    let data = U256::from_big_endian(&data);

    // For long data, it is better to use hex-encoding for greater readability
    let data_str = if data > U256::from(u64::MAX) {
        let mut bytes = [0u8; 32];
        data.to_big_endian(&mut bytes);
        format!("0x{}", hex::encode(bytes))
    } else {
        data.to_string()
    };
    let tx_id = state.vm_local_state.tx_number_in_block;
    format!("Bootloader transaction {tx_id}: {msg}: {data_str}")
}

/// Reads the memory slice represented by the fat pointer.
/// Note, that the fat pointer must point to the accessible memory (i.e. not cleared up yet).
pub(crate) fn read_pointer<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    pointer: FatPointer,
) -> Vec<u8> {
    let FatPointer {
        offset,
        length,
        start,
        memory_page,
    } = pointer;

    // The actual bounds of the returndata ptr is [start+offset..start+length]
    let mem_region_start = start + offset;
    let mem_region_length = length - offset;

    memory.read_unaligned_bytes(
        memory_page as usize,
        mem_region_start as usize,
        mem_region_length as usize,
    )
}

/// Outputs the returndata for the latest call.
/// This is usually used to output the revert reason.
pub(crate) fn get_debug_returndata<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    latest_returndata_ptr: Option<FatPointer>,
) -> String {
    let returndata = if let Some(ptr) = latest_returndata_ptr {
        read_pointer(memory, ptr)
    } else {
        vec![]
    };

    format!("0x{}", hex::encode(returndata))
}

/// Accepts a vm hook and, if it requires to output some debug log, outputs it.
pub(crate) fn print_debug_if_needed<H: HistoryMode>(
    hook: &VmHook,
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
    latest_returndata_ptr: Option<FatPointer>,
    subversion: MultiVMSubversion,
) {
    let log = match hook {
        VmHook::DebugLog => get_debug_log(state, memory, subversion),
        VmHook::DebugReturnData => get_debug_returndata(memory, latest_returndata_ptr),
        _ => return,
    };
    tracing::trace!("{log}");
}

pub(crate) fn get_calldata_page_via_abi(far_call_abi: &FarCallABI, base_page: MemoryPage) -> u32 {
    match far_call_abi.forwarding_mode {
        FarCallForwardPageType::ForwardFatPointer => {
            far_call_abi.memory_quasi_fat_pointer.memory_page
        }
        FarCallForwardPageType::UseAuxHeap => aux_heap_page_from_base(base_page).0,
        FarCallForwardPageType::UseHeap => heap_page_from_base(base_page).0,
    }
}
pub(crate) fn get_vm_hook_params<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    subversion: MultiVMSubversion,
) -> Vec<U256> {
    let start_position = get_vm_hook_params_start_position(subversion);
    memory.dump_page_content_as_u256_words(
        BOOTLOADER_HEAP_PAGE,
        start_position..start_position + VM_HOOK_PARAMS_COUNT,
    )
}
