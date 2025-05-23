use circuit_sequencer_api::{BLOB_CHUNK_SIZE, ELEMENTS_PER_4844_BLOCK};
use zk_evm_1_5_2::aux_structures::MemoryPage;
pub use zk_evm_1_5_2::zkevm_opcode_defs::system_params::{
    ERGS_PER_CIRCUIT, INITIAL_STORAGE_WRITE_PUBDATA_BYTES,
};

use crate::vm_latest::{old_vm::utils::heap_page_from_base, MultiVmSubversion};

/// The amount of ergs to be reserved at the end of the batch to ensure that it has enough ergs to verify compression, etc.
pub(crate) const BOOTLOADER_BATCH_TIP_OVERHEAD: u32 = 400_000_000;

pub(crate) const BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD: u32 = 12_000;
pub(crate) const BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD: u32 = 2000;

/// In the version `1.5.2` the maximal number of circuits per batch has been decreased to '28000'.
// Please see scheduler capacity in https://github.com/matter-labs/zksync-protocol/blob/main/crates/circuit_definitions/src/circuit_definitions/recursion_layer/mod.rs#L38
pub(crate) const MAX_BASE_LAYER_CIRCUITS: usize = 28_000;

/// The size of the bootloader memory in bytes which is used by the protocol.
/// While the maximal possible size is a lot higher, we restrict ourselves to a certain limit to reduce
/// the requirements on RAM.
/// In this version of the VM the used bootloader memory bytes has increased from `30_000_000` to `59_000_000`,
/// and then to `63_800_000` in a subsequent upgrade.
pub(crate) const fn get_used_bootloader_memory_bytes(subversion: MultiVmSubversion) -> usize {
    match subversion {
        MultiVmSubversion::SmallBootloaderMemory => 59_000_000,
        MultiVmSubversion::IncreasedBootloaderMemory
        | MultiVmSubversion::Gateway
        | MultiVmSubversion::EvmEmulator
        | MultiVmSubversion::EcPrecompiles => 63_800_000,
    }
}

pub(crate) const fn get_used_bootloader_memory_words(subversion: MultiVmSubversion) -> usize {
    get_used_bootloader_memory_bytes(subversion) / 32
}

/// We want `MAX_GAS_PER_PUBDATA_BYTE` multiplied by the u32::MAX (i.e. the maximal possible value of the pubdata counter)
/// to be a safe integer with a good enough margin.
/// Note, that in the version `1.5.0` we've only increased the max gas per pubdata byte to 50k for backward compatibility with the
/// old SDK, where the default signed value is 50k.
pub(crate) const MAX_GAS_PER_PUBDATA_BYTE: u64 = 50_000;

// The maximal number of transactions in a single batch.
// In this version of the VM the limit has been increased from `1024` to to `10000`.
pub(crate) const MAX_TXS_IN_BATCH: usize = 10000;

/// Max cycles for a single transaction.
pub const MAX_CYCLES_FOR_TX: u32 = u32::MAX;

/// The first 32 slots are reserved for debugging purposes
pub(crate) const DEBUG_SLOTS_OFFSET: usize = 8;
pub(crate) const DEBUG_FIRST_SLOTS: usize = 32;

/// The next 33 slots are reserved for dealing with the paymaster context (1 slot for storing length + 32 slots for storing the actual context).
pub(crate) const PAYMASTER_CONTEXT_SLOTS: usize = 32 + 1;
/// The next PAYMASTER_CONTEXT_SLOTS + 7 slots free slots are needed before each tx, so that the
/// postOp operation could be encoded correctly.
pub(crate) const MAX_POSTOP_SLOTS: usize = PAYMASTER_CONTEXT_SLOTS + 7;

/// Slots used to store the current L2 transaction's hash and the hash recommended
/// to be used for signing the transaction's content.
const CURRENT_L2_TX_HASHES_SLOTS: usize = 2;

pub(crate) const fn get_max_new_factory_deps(subversion: MultiVmSubversion) -> usize {
    match subversion {
        MultiVmSubversion::SmallBootloaderMemory | MultiVmSubversion::IncreasedBootloaderMemory => {
            32
        }
        // With gateway upgrade we increased max number of factory dependencies
        MultiVmSubversion::Gateway
        | MultiVmSubversion::EvmEmulator
        | MultiVmSubversion::EcPrecompiles => 64,
    }
}

/// Slots used to store the calldata for the KnownCodesStorage to mark new factory
/// dependencies as known ones. Besides the slots for the new factory dependencies themselves
/// another 4 slots are needed for: selector, marker of whether the user should pay for the pubdata,
/// the offset for the encoding of the array as well as the length of the array.
pub(crate) const fn get_new_factory_deps_reserved_slots(subversion: MultiVmSubversion) -> usize {
    get_max_new_factory_deps(subversion) + 4
}

/// The operator can provide for each transaction the proposed minimal refund
pub(crate) const OPERATOR_REFUNDS_SLOTS: usize = MAX_TXS_IN_BATCH;

pub(crate) const fn get_operator_refunds_offset(subversion: MultiVmSubversion) -> usize {
    DEBUG_SLOTS_OFFSET
        + DEBUG_FIRST_SLOTS
        + PAYMASTER_CONTEXT_SLOTS
        + CURRENT_L2_TX_HASHES_SLOTS
        + get_new_factory_deps_reserved_slots(subversion)
}

pub(crate) const fn get_tx_overhead_offset(subversion: MultiVmSubversion) -> usize {
    get_operator_refunds_offset(subversion) + OPERATOR_REFUNDS_SLOTS
}

pub(crate) const TX_OVERHEAD_SLOTS: usize = MAX_TXS_IN_BATCH;

pub(crate) const fn get_tx_trusted_gas_limit_offset(subversion: MultiVmSubversion) -> usize {
    get_tx_overhead_offset(subversion) + TX_OVERHEAD_SLOTS
}

pub(crate) const TX_TRUSTED_GAS_LIMIT_SLOTS: usize = MAX_TXS_IN_BATCH;

pub(crate) const fn get_tx_operator_l2_block_info_offset(subversion: MultiVmSubversion) -> usize {
    get_tx_trusted_gas_limit_offset(subversion) + TX_TRUSTED_GAS_LIMIT_SLOTS
}

pub(crate) const TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO: usize = 4;
pub(crate) const TX_OPERATOR_L2_BLOCK_INFO_SLOTS: usize =
    (MAX_TXS_IN_BATCH + 1) * TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO;

pub(crate) const fn get_compressed_bytecodes_offset(subversion: MultiVmSubversion) -> usize {
    get_tx_operator_l2_block_info_offset(subversion) + TX_OPERATOR_L2_BLOCK_INFO_SLOTS
}

pub(crate) const COMPRESSED_BYTECODES_SLOTS: usize = 196608;

pub(crate) const fn get_priority_txs_l1_data_offset(subversion: MultiVmSubversion) -> usize {
    get_compressed_bytecodes_offset(subversion) + COMPRESSED_BYTECODES_SLOTS
}

pub(crate) const PRIORITY_TXS_L1_DATA_SLOTS: usize = 2;

pub(crate) const fn get_operator_provided_l1_messenger_pubdata_offset(
    subversion: MultiVmSubversion,
) -> usize {
    get_priority_txs_l1_data_offset(subversion) + PRIORITY_TXS_L1_DATA_SLOTS
}

/// One of "worst case" scenarios for the number of state diffs in a batch is when 780kb of pubdata is spent
/// on repeated writes, that are all zeroed out. In this case, the number of diffs is `780kb / 5 = 156k`. This means that they will have
/// accommodate 42432000 bytes of calldata for the uncompressed state diffs. Adding 780kb on top leaves us with
/// roughly 43212000 bytes needed for calldata.
/// 1350375 slots are needed to accommodate this amount of data. We round up to 1360000 slots just in case.
///
/// In theory though much more calldata could be used (if for instance 1 byte is used for enum index). It is the responsibility of the
/// operator to ensure that it can form the correct calldata for the L1Messenger.
pub(crate) const OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS: usize = 1360000;

pub(crate) const fn get_bootloader_tx_description_offset(subversion: MultiVmSubversion) -> usize {
    get_operator_provided_l1_messenger_pubdata_offset(subversion)
        + OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS
}

/// The size of the bootloader memory dedicated to the encodings of transactions
pub(crate) const fn get_bootloader_tx_encoding_space(subversion: MultiVmSubversion) -> u32 {
    (get_used_bootloader_memory_words(subversion)
        - get_tx_description_offset(subversion)
        - MAX_TXS_IN_BATCH) as u32
}

// Size of the bootloader tx description in words
pub(crate) const BOOTLOADER_TX_DESCRIPTION_SIZE: usize = 2;

/// The actual descriptions of transactions should start after the minor descriptions and a MAX_POSTOP_SLOTS
/// free slots to allow postOp encoding.
pub(crate) const fn get_tx_description_offset(subversion: MultiVmSubversion) -> usize {
    get_bootloader_tx_description_offset(subversion)
        + BOOTLOADER_TX_DESCRIPTION_SIZE * MAX_TXS_IN_BATCH
        + MAX_POSTOP_SLOTS
}

pub(crate) const TX_GAS_LIMIT_OFFSET: usize = 4;

const INITIAL_BASE_PAGE: u32 = 8;
pub const BOOTLOADER_HEAP_PAGE: u32 = heap_page_from_base(MemoryPage(INITIAL_BASE_PAGE)).0;

/// VM Hooks are used for communication between bootloader and tracers.
///
/// The 'type' / 'opcode' is put into VM_HOOK_POSITION slot,
/// and VM_HOOKS_PARAMS_COUNT parameters (each 32 bytes) are put in the slots before.
/// So the layout looks like this:
/// `[param 0][param 1][param 2][vmhook opcode]`
pub const VM_HOOK_PARAMS_COUNT: u32 = 3;
pub(crate) const fn get_vm_hook_position(subversion: MultiVmSubversion) -> u32 {
    get_result_success_first_slot(subversion) - 1
}
pub(crate) const fn get_vm_hook_params_start_position(subversion: MultiVmSubversion) -> u32 {
    get_vm_hook_position(subversion) - VM_HOOK_PARAMS_COUNT
}

/// Method that provides the start position of the vm hook in the memory for the latest version of v1.5.0.
///
/// This method is used only in `test_infra` in the bootloader tests and that's why it should be exposed.
pub const fn get_vm_hook_start_position_latest() -> u32 {
    get_vm_hook_params_start_position(MultiVmSubversion::IncreasedBootloaderMemory)
}

/// Arbitrary space in memory closer to the end of the page
pub(crate) const fn get_result_success_first_slot(subversion: MultiVmSubversion) -> u32 {
    ((get_used_bootloader_memory_bytes(subversion) as u32) - (MAX_TXS_IN_BATCH as u32) * 32) / 32
}

/// How many gas bootloader is allowed to spend within one block.
///
/// Note that this value doesn't correspond to the gas limit of any particular transaction
/// (except for the fact that, of course, gas limit for each transaction should be <= `BLOCK_GAS_LIMIT`).
pub const BATCH_COMPUTATIONAL_GAS_LIMIT: u32 =
    zk_evm_1_5_2::zkevm_opcode_defs::system_params::VM_INITIAL_FRAME_ERGS;

/// The maximal number of gas that is supposed to be spent in a batch. This value is displayed in the system context as well
/// as the API for each batch.
///
/// Using any number that fits into `i64` is fine with regard to any popular eth node implementation, but we also desire to use
/// values that fit into safe JS numbers just in case for compatibility.
pub const BATCH_GAS_LIMIT: u64 = 1 << 50;

/// How many gas is allowed to spend on a single transaction in eth_call method
pub const ETH_CALL_GAS_LIMIT: u64 = BATCH_GAS_LIMIT;

/// ID of the transaction from L1
pub const L1_TX_TYPE: u8 = 255;

/// The maximal gas limit that gets passed as compute for an L2 transaction. This is also the maximal limit allowed
/// for L1->L2 transactions.
///
/// It is equal to the `MAX_GAS_PER_TRANSACTION` in the bootloader.
/// For L2 transaction we need to limit the amount of gas that can be spent on compute while the rest can only be spent on pubdata.
/// For L1->L2 transactions We need to limit the number of gas that can be passed into the L1->L2 transaction due to the fact
/// that unlike L2 transactions they can not be rejected by the operator and must be executed. In turn,
/// this means that if a transaction spends more than `MAX_GAS_PER_TRANSACTION`, it use up all the limited resources of a batch.
pub(crate) const TX_MAX_COMPUTE_GAS_LIMIT: usize = 80_000_000;

/// The amount of gas to be charged for occupying a single slot of a transaction.
/// It is roughly equal to `80kk/MAX_TRANSACTIONS_PER_BATCH`, i.e. how many gas would an L1->L2 transaction
/// need to pay to compensate for the batch being closed.
/// While the derived formula is used for the worst case for L1->L2 transaction, it suits L2 transactions as well
/// and serves to compensate the operator for the fact that they need to process the transaction. In case batches start
/// getting often sealed due to the slot limit being reached, the L2 fair gas price will be increased.
pub(crate) const TX_SLOT_OVERHEAD_GAS: u32 = 10_000;

/// The amount of gas to be charged for occupying a single byte of the bootloader's memory.
/// It is roughly equal to `80kk/BOOTLOADER_MEMORY_FOR_TXS`, i.e. how many gas would an L1->L2 transaction
/// need to pay to compensate for the batch being closed.
/// While the derived formula is used for the worst case for L1->L2 transaction, it suits L2 transactions as well
/// and serves to compensate the operator for the fact that they need to fill up the bootloader memory. In case batches start
/// getting often sealed due to the memory limit being reached, the L2 fair gas price will be increased.
pub(crate) const TX_MEMORY_OVERHEAD_GAS: u32 = 10;

pub(crate) const ZK_SYNC_BYTES_PER_BLOB: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;
pub const MAX_BLOBS_PER_BATCH: usize = 6;
pub const MAX_VM_PUBDATA_PER_BATCH: usize = MAX_BLOBS_PER_BATCH * ZK_SYNC_BYTES_PER_BLOB;
