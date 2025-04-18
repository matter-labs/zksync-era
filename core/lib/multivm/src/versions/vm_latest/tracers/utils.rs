use zk_evm_1_5_2::{
    aux_structures::MemoryPage,
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{
        FarCallABI, FarCallForwardPageType, FatPointer, LogOpcode, Opcode, UMAOpcode,
    },
};
use zksync_system_constants::{
    ECRECOVER_PRECOMPILE_ADDRESS, KECCAK256_PRECOMPILE_ADDRESS,
    SECP256R1_VERIFY_PRECOMPILE_ADDRESS, SHA256_PRECOMPILE_ADDRESS,
};
use zksync_types::{u256_to_h256, U256};

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
    vm::MultiVmSubversion,
    VmHook,
};

impl VmHook {
    pub(crate) fn from_opcode_memory(
        state: &VmLocalStateData<'_>,
        data: &BeforeExecutionData,
        subversion: MultiVmSubversion,
    ) -> Option<Self> {
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
            return None;
        }

        Some(Self::new(value.as_u32()))
    }
}

pub(crate) fn print_debug_log<H: HistoryMode>(
    state: &VmLocalStateData<'_>,
    memory: &SimpleMemory<H>,
    subversion: MultiVmSubversion,
) {
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
    tracing::trace!("Bootloader transaction {tx_id}: {msg}: {data_str}");
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
pub(crate) fn print_debug_returndata<H: HistoryMode>(
    memory: &SimpleMemory<H>,
    latest_returndata_ptr: Option<FatPointer>,
) {
    let returndata = if let Some(ptr) = latest_returndata_ptr {
        read_pointer(memory, ptr)
    } else {
        vec![]
    };

    tracing::trace!("0x{}", hex::encode(returndata));
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
                || address == SECP256R1_VERIFY_PRECOMPILE_ADDRESS
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
    subversion: MultiVmSubversion,
) -> Vec<U256> {
    let start_position = get_vm_hook_params_start_position(subversion);
    memory.dump_page_content_as_u256_words(
        BOOTLOADER_HEAP_PAGE,
        start_position..start_position + VM_HOOK_PARAMS_COUNT,
    )
}
