use circuit_sequencer_api::INITIAL_MONOTONIC_CYCLE_COUNTER;
use zk_evm_1_3_3::{
    aux_structures::{MemoryPage, Timestamp},
    block_properties::BlockProperties,
    vm_state::{CallStackEntry, PrimitiveValue, VmState},
    witness_trace::DummyTracer,
    zkevm_opcode_defs::{
        system_params::{BOOTLOADER_MAX_MEMORY, INITIAL_FRAME_FORMAL_EH_LOCATION},
        FatPointer, BOOTLOADER_BASE_PAGE, BOOTLOADER_CALLDATA_PAGE, BOOTLOADER_CODE_PAGE,
        STARTING_BASE_PAGE, STARTING_TIMESTAMP,
    },
};
use zksync_system_constants::BOOTLOADER_ADDRESS;
use zksync_types::{block::L2BlockHasher, h256_to_u256, Address, L2BlockNumber};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        L1BatchEnv, L2Block, SystemEnv,
    },
    utils::bytecode::bytes_to_be_words,
    vm_virtual_blocks::{
        bootloader_state::BootloaderState,
        constants::BOOTLOADER_HEAP_PAGE,
        old_vm::{
            event_sink::InMemoryEventSink,
            history_recorder::HistoryMode,
            memory::SimpleMemory,
            oracles::{
                decommitter::DecommitterOracle, precompile::PrecompilesProcessorWithHistory,
                storage::StorageOracle,
            },
        },
        types::l1_batch_env::bootloader_initial_memory,
        utils::l2_blocks::{assert_next_block, load_last_l2_block},
    },
};

pub type ZkSyncVmState<S, H> = VmState<
    StorageOracle<S, H>,
    SimpleMemory<H>,
    InMemoryEventSink<H>,
    PrecompilesProcessorWithHistory<false, H>,
    DecommitterOracle<false, S, H>,
    DummyTracer,
>;

fn formal_calldata_abi() -> PrimitiveValue {
    let fat_pointer = FatPointer {
        offset: 0,
        memory_page: BOOTLOADER_CALLDATA_PAGE,
        start: 0,
        length: 0,
    };

    PrimitiveValue {
        value: fat_pointer.to_u256(),
        is_pointer: true,
    }
}

/// Initialize the vm state and all necessary oracles
pub(crate) fn new_vm_state<S: WriteStorage, H: HistoryMode>(
    storage: StoragePtr<S>,
    system_env: &SystemEnv,
    l1_batch_env: &L1BatchEnv,
) -> (ZkSyncVmState<S, H>, BootloaderState) {
    let last_l2_block = if let Some(last_l2_block) = load_last_l2_block(storage.clone()) {
        last_l2_block
    } else {
        // This is the scenario of either the first L2 block ever or
        // the first block after the upgrade for support of L2 blocks.
        L2Block {
            number: l1_batch_env.first_l2_block.number.saturating_sub(1),
            timestamp: 0,
            hash: L2BlockHasher::legacy_hash(L2BlockNumber(l1_batch_env.first_l2_block.number) - 1),
        }
    };

    assert_next_block(&last_l2_block, &l1_batch_env.first_l2_block);
    let first_l2_block = l1_batch_env.first_l2_block;
    let storage_oracle: StorageOracle<S, H> = StorageOracle::new(storage.clone());
    let mut memory = SimpleMemory::default();
    let event_sink = InMemoryEventSink::default();
    let precompiles_processor = PrecompilesProcessorWithHistory::<false, H>::default();
    let mut decommittment_processor: DecommitterOracle<false, S, H> =
        DecommitterOracle::new(storage);

    decommittment_processor.populate(
        vec![(
            h256_to_u256(system_env.base_system_smart_contracts.default_aa.hash),
            bytes_to_be_words(&system_env.base_system_smart_contracts.default_aa.code),
        )],
        Timestamp(0),
    );

    memory.populate(
        vec![(
            BOOTLOADER_CODE_PAGE,
            bytes_to_be_words(&system_env.base_system_smart_contracts.bootloader.code),
        )],
        Timestamp(0),
    );

    let bootloader_initial_memory = bootloader_initial_memory(l1_batch_env);
    memory.populate_page(
        BOOTLOADER_HEAP_PAGE as usize,
        bootloader_initial_memory.clone(),
        Timestamp(0),
    );

    let mut vm = VmState::empty_state(
        storage_oracle,
        memory,
        event_sink,
        precompiles_processor,
        decommittment_processor,
        DummyTracer,
        BlockProperties {
            default_aa_code_hash: h256_to_u256(
                system_env.base_system_smart_contracts.default_aa.hash,
            ),
            zkporter_is_available: system_env.zk_porter_available,
        },
    );

    vm.local_state.callstack.current.ergs_remaining = system_env.bootloader_gas_limit;

    let initial_context = CallStackEntry {
        this_address: BOOTLOADER_ADDRESS,
        msg_sender: Address::zero(),
        code_address: BOOTLOADER_ADDRESS,
        base_memory_page: MemoryPage(BOOTLOADER_BASE_PAGE),
        code_page: MemoryPage(BOOTLOADER_CODE_PAGE),
        sp: 0,
        pc: 0,
        // Note, that since the results are written at the end of the memory
        // it is needed to have the entire heap available from the beginning
        heap_bound: BOOTLOADER_MAX_MEMORY,
        aux_heap_bound: BOOTLOADER_MAX_MEMORY,
        exception_handler_location: INITIAL_FRAME_FORMAL_EH_LOCATION,
        ergs_remaining: system_env.bootloader_gas_limit,
        this_shard_id: 0,
        caller_shard_id: 0,
        code_shard_id: 0,
        is_static: false,
        is_local_frame: false,
        context_u128_value: 0,
    };

    // We consider the contract that is being run as a bootloader
    vm.push_bootloader_context(INITIAL_MONOTONIC_CYCLE_COUNTER - 1, initial_context);
    vm.local_state.timestamp = STARTING_TIMESTAMP;
    vm.local_state.memory_page_counter = STARTING_BASE_PAGE;
    vm.local_state.monotonic_cycle_counter = INITIAL_MONOTONIC_CYCLE_COUNTER;
    vm.local_state.current_ergs_per_pubdata_byte = 0;
    vm.local_state.registers[0] = formal_calldata_abi();

    // Deleting all the historical records brought by the initial
    // initialization of the VM to make them permanent.
    vm.decommittment_processor.delete_history();
    vm.event_sink.delete_history();
    vm.storage.delete_history();
    vm.memory.delete_history();
    vm.precompiles_processor.delete_history();
    let bootloader_state = BootloaderState::new(
        system_env.execution_mode,
        bootloader_initial_memory,
        first_l2_block,
    );

    (vm, bootloader_state)
}
