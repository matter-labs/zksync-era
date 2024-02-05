use std::marker::PhantomData;

use itertools::Itertools;
use zk_evm_1_4_1::aux_structures::LogQuery as LogQuery_1_4_1;
use zk_evm_1_5_0::{
    aux_structures::{LogQuery, Timestamp},
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{FatPointer, Opcode, UMAOpcode},
};
use zkevm_test_harness_1_4_1::witness::sort_storage_access::sort_storage_access_queries;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    event::{
        extract_bytecode_publication_requests_from_l1_messenger,
        extract_l2tol1logs_from_l1_messenger, extract_long_l2_to_l1_messages, L1MessengerL2ToL1Log,
    },
    writes::StateDiffRecord,
    AccountTreeId, Address, StorageKey, L1_MESSENGER_ADDRESS, U256,
};
use zksync_utils::{h256_to_u256, u256_to_bytes_be, u256_to_h256};

use crate::{
    interface::{dyn_tracers::vm_1_5_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{
        old_vm::utils::heap_page_from_base, BootloaderState, HistoryMode, SimpleMemory, VmTracer,
        ZkSyncVmState,
    },
};

pub(crate) struct EvmDebugTracer {
    address: Address,
    counter: u32,
}

impl EvmDebugTracer {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            counter: 0,
        }
    }
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for EvmDebugTracer {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        // FIXME: this catches not only Evm contracts

        let code_address = state.vm_local_state.callstack.current.code_address;
        let opcode_variant = data.opcode.variant;
        let heap_page =
            heap_page_from_base(state.vm_local_state.callstack.current.base_memory_page).0;

        let src0_value = data.src0_value.value;

        let fat_ptr = FatPointer::from_u256(src0_value);

        let value = data.src1_value.value;

        const DEBUG_SLOT: u32 = 32 * 32;
        const STACK_POINT: u32 = DEBUG_SLOT + 32 * 5;
        const BYTECODE_OFFSET: u32 = STACK_POINT + 1024;

        let debug_magic = U256::from_dec_str(
            "33509158800074003487174289148292687789659295220513886355337449724907776218753",
        )
        .unwrap();

        // Only `UMA` opcodes in the bootloader serve for vm hooks
        if !matches!(opcode_variant.opcode, Opcode::UMA(UMAOpcode::HeapWrite))
            || fat_ptr.offset != DEBUG_SLOT
            || value != debug_magic
        {
            // println!("I tried");
            return;
        }

        let bytecode_len = memory
            .read_slot(heap_page as usize, BYTECODE_OFFSET as usize / 32)
            .value;
        let ip = memory.read_slot(heap_page as usize, 32 + 1).value;
        let tos = memory.read_slot(heap_page as usize, 32 + 2).value;
        let gasleft = memory.read_slot(heap_page as usize, 32 + 3).value;
        let true_opcode = memory.read_slot(heap_page as usize, 32 + 4).value.as_u32() as u8;

        let opcode_id = memory.read_unaligned_bytes(heap_page as usize, ip.as_usize(), 1);

        let mut stack = vec![];
        for i in 0..16 {
            let point = tos - i * 32;
            if point >= U256::from(STACK_POINT) {
                stack.push(
                    memory
                        .read_slot(heap_page as usize, point.as_usize() / 32)
                        .value,
                );
            }
        }

        self.counter += 1;

        println!(
            "EVM execution at {}. TOS: {}, IP: {} (max: {}), OPCODE: 0x{}/0x{}, GASLEFT: {gasleft}, COUNTER: {}\nStack: {:#?}\n",
            hex::encode(&code_address.0),
            tos,
            ip,
            bytecode_len,
            hex::encode(&opcode_id),
            hex::encode(&[true_opcode]),
            self.counter,
            stack
        );
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for EvmDebugTracer {
    // fn finish_cycle(
    //         &mut self,
    //         _state: &mut ZkSyncVmState<S, H>,
    //         _bootloader_state: &mut BootloaderState,
    // ) -> TracerExecutionStatus {
    //     let contract = _state.local_state.callstack.current.code_address;

    // }
}
