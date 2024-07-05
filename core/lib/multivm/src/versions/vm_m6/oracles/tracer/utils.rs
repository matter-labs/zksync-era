use zk_evm_1_3_1::{
    abstractions::{BeforeExecutionData, VmLocalStateData},
    aux_structures::MemoryPage,
    zkevm_opcode_defs::{
        FarCallABI, FarCallForwardPageType, FatPointer, LogOpcode, Opcode, UMAOpcode,
    },
};
use zksync_system_constants::{
    ECRECOVER_PRECOMPILE_ADDRESS, KECCAK256_PRECOMPILE_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS,
    L1_MESSENGER_ADDRESS, SHA256_PRECOMPILE_ADDRESS,
};
use zksync_types::U256;
use zksync_utils::u256_to_h256;

use crate::vm_m6::{
    history_recorder::HistoryMode,
    memory::SimpleMemory,
    utils::{aux_heap_page_from_base, heap_page_from_base},
    vm_instance::{get_vm_hook_params, VM_HOOK_POSITION},
    vm_with_bootloader::BOOTLOADER_HEAP_PAGE,
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
}

impl VmHook {
    pub fn from_opcode_memory(state: &VmLocalStateData<'_>, data: &BeforeExecutionData) -> Self {
        let opcode_variant = data.opcode.variant;
        let heap_page =
            heap_page_from_base(state.vm_local_state.callstack.current.base_memory_page).0;

        let src0_value = data.src0_value.value;

        let fat_ptr = FatPointer::from_u256(src0_value);

        let value = data.src1_value.value;

        // Only `UMA` opcodes in the bootloader serve for vm hooks
        if !matches!(opcode_variant.opcode, Opcode::UMA(UMAOpcode::HeapWrite))
            || heap_page != BOOTLOADER_HEAP_PAGE
            || fat_ptr.offset != VM_HOOK_POSITION * 32
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
            _ => panic!("Unknown hook: {}", value.as_u32()),
        }
    }
}

pub(crate) fn get_debug_log<H: HistoryMode>(
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
) -> String {
    let vm_hook_params: Vec<_> = get_vm_hook_params(memory)
        .into_iter()
        .map(u256_to_h256)
        .collect();
    let msg = vm_hook_params[0].as_bytes().to_vec();
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

    format!("Bootloader transaction {}: {} {}", tx_id, msg, data_str)
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
pub(crate) fn get_debug_returndata<H: HistoryMode>(memory: &SimpleMemory<H>) -> String {
    let vm_hook_params: Vec<_> = get_vm_hook_params(memory);
    let returndata_ptr = FatPointer::from_u256(vm_hook_params[0]);
    let returndata = read_pointer(memory, returndata_ptr);

    format!("0x{}", hex::encode(returndata))
}

/// Accepts a vm hook and, if it requires to output some debug log, outputs it.
pub(crate) fn print_debug_if_needed<H: HistoryMode>(
    hook: &VmHook,
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
) {
    let log = match hook {
        VmHook::DebugLog => get_debug_log(state, memory),
        VmHook::DebugReturnData => get_debug_returndata(memory),
        _ => return,
    };

    tracing::trace!("{}", log);
}

pub(crate) fn computational_gas_price(
    state: VmLocalStateData<'_>,
    data: &BeforeExecutionData,
) -> u32 {
    // We calculate computational gas used as a raw price for opcode plus cost for precompiles.
    // This calculation is incomplete as it misses decommitment and memory growth costs.
    // To calculate decommitment cost we need an access to decommitter oracle which is missing in tracer now.
    // Memory growth calculation is complex and it will require different logic for different opcodes (`FarCall`, `Ret`, `UMA`).
    let base_price = data.opcode.inner.variant.ergs_price();
    let precompile_price = match data.opcode.variant.opcode {
        Opcode::Log(LogOpcode::PrecompileCall) => {
            let address = state.vm_local_state.callstack.current.this_address;

            if address == KECCAK256_PRECOMPILE_ADDRESS
                || address == SHA256_PRECOMPILE_ADDRESS
                || address == ECRECOVER_PRECOMPILE_ADDRESS
            {
                data.src1_value.value.low_u32()
            } else {
                0
            }
        }
        _ => 0,
    };
    base_price + precompile_price
}

pub(crate) fn gas_spent_on_bytecodes_and_long_messages_this_opcode(
    state: &VmLocalStateData<'_>,
    data: &BeforeExecutionData,
) -> u32 {
    if data.opcode.variant.opcode == Opcode::Log(LogOpcode::PrecompileCall) {
        let current_stack = state.vm_local_state.callstack.get_current_stack();
        // Trace for precompile calls from `KNOWN_CODES_STORAGE_ADDRESS` and `L1_MESSENGER_ADDRESS` that burn some gas.
        // Note, that if there is less gas left than requested to burn it will be burnt anyway.
        if current_stack.this_address == KNOWN_CODES_STORAGE_ADDRESS
            || current_stack.this_address == L1_MESSENGER_ADDRESS
        {
            std::cmp::min(data.src1_value.value.as_u32(), current_stack.ergs_remaining)
        } else {
            0
        }
    } else {
        0
    }
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
