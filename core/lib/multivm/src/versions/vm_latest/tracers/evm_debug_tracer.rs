use std::{marker::PhantomData, str::FromStr};

use circuit_sequencer_api_1_4_2::sort_storage_access::sort_storage_access_queries;
use itertools::Itertools;
use zk_evm_1_4_1::aux_structures::LogQuery as LogQuery_1_4_1;
use zk_evm_1_5_0::{
    aux_structures::{LogQuery, Timestamp},
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{FatPointer, Opcode, UMAOpcode},
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{
    event::{
        extract_bytecode_publication_requests_from_l1_messenger,
        extract_l2tol1logs_from_l1_messenger, extract_long_l2_to_l1_messages, L1MessengerL2ToL1Log,
    },
    writes::StateDiffRecord,
    AccountTreeId, Address, StorageKey, H256, L1_MESSENGER_ADDRESS, U256,
};
use zksync_utils::{h256_to_u256, u256_to_bytes_be, u256_to_h256};

use crate::{
    interface::{dyn_tracers::vm_1_5_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{
        old_vm::utils::{heap_page_from_base, IntoFixedLengthByteIterator},
        BootloaderState, HistoryMode, SimpleMemory, VmTracer, ZkSyncVmState,
    },
};

pub(crate) struct EvmDebugTracer<S: WriteStorage, H: HistoryMode> {
    counter: u32,

    _storage: PhantomData<S>,
    _history: PhantomData<H>,
}

impl<S: WriteStorage, H: HistoryMode> EvmDebugTracer<S, H> {
    pub fn new() -> Self {
        Self {
            counter: 0,
            _storage: PhantomData,
            _history: PhantomData,
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for EvmDebugTracer<S, H> {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        // FIXME: this catches not only Evm contracts

        let opcode_variant = data.opcode.variant;
        let heap_page =
            heap_page_from_base(state.vm_local_state.callstack.current.base_memory_page).0;

        let src0_value = data.src0_value.value;

        let fat_ptr = FatPointer::from_u256(src0_value);

        let value = data.src1_value.value;

        const DEBUG_SLOT: u32 = 32 * 32;

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

        let how_to_print_value = memory.read_slot(heap_page as usize, 32 + 1).value;
        let value_to_print = memory.read_slot(heap_page as usize, 32 + 2).value;

        let print_as_hex_value =
            U256::from_str("0x00debdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebde")
                .unwrap();
        let print_as_string_value =
            U256::from_str("0x00debdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdebdf")
                .unwrap();

        if how_to_print_value == print_as_hex_value {
            print!("PRINTED: ");
            println!("0x{:02x}", value_to_print);
        }

        if how_to_print_value == print_as_string_value {
            print!("PRINTED: ");
            let mut value = value_to_print.0;
            value.reverse();
            for limb in value {
                print!(
                    "{}",
                    String::from_utf8(limb.to_be_bytes().to_vec()).unwrap()
                );
            }
            println!("");
        }

        self.counter += 1;
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for EvmDebugTracer<S, H> {
    // fn finish_cycle(
    //         &mut self,
    //         _state: &mut ZkSyncVmState<S, H>,
    //         _bootloader_state: &mut BootloaderState,
    // ) -> TracerExecutionStatus {
    //     let contract = _state.local_state.callstack.current.code_address;

    // }
}
