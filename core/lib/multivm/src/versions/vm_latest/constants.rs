use circuit_sequencer_api_1_4_2::{BLOB_CHUNK_SIZE, ELEMENTS_PER_4844_BLOCK};
use zk_evm_1_5_0::aux_structures::MemoryPage;
pub use zk_evm_1_5_0::zkevm_opcode_defs::system_params::{
    ERGS_PER_CIRCUIT, INITIAL_STORAGE_WRITE_PUBDATA_BYTES,
};
use zksync_system_constants::{MAX_L2_TX_GAS_LIMIT, MAX_NEW_FACTORY_DEPS};

use crate::vm_latest::old_vm::utils::heap_page_from_base;

/// The amount of ergs to be reserved at the end of the batch to ensure that it has enough ergs to verify compression, etc.
pub(crate) const BOOTLOADER_BATCH_TIP_OVERHEAD: u32 = 170_000_000;

#[allow(dead_code)]
pub(crate) const BOOTLOADER_BATCH_TIP_CIRCUIT_STATISTICS_OVERHEAD: u64 = 5000;
#[allow(dead_code)]
pub(crate) const BOOTLOADER_BATCH_TIP_METRICS_SIZE_OVERHEAD: u64 = 1500;

/// The size of the bootloader memory in bytes which is used by the protocol.
/// While the maximal possible size is a lot higher, we restrict ourselves to a certain limit to reduce
/// the requirements on RAM.
/// In this version of the VM the used bootloader memory bytes has increased from `30_000_000` to `59_000_000`.
pub(crate) const USED_BOOTLOADER_MEMORY_BYTES: usize = 59_000_000;
pub(crate) const USED_BOOTLOADER_MEMORY_WORDS: usize = USED_BOOTLOADER_MEMORY_BYTES / 32;

/// We want `MAX_GAS_PER_PUBDATA_BYTE` multiplied by the u32::MAX (i.e. the maximal possible value of the pubdata counter)
/// to be a safe integer with a good enough margin.
pub(crate) const MAX_GAS_PER_PUBDATA_BYTE: u64 = 1 << 20;

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

/// Slots used to store the calldata for the KnownCodesStorage to mark new factory
/// dependencies as known ones. Besides the slots for the new factory dependencies themselves
/// another 4 slots are needed for: selector, marker of whether the user should pay for the pubdata,
/// the offset for the encoding of the array as well as the length of the array.
const NEW_FACTORY_DEPS_RESERVED_SLOTS: usize = MAX_NEW_FACTORY_DEPS + 4;

/// The operator can provide for each transaction the proposed minimal refund
pub(crate) const OPERATOR_REFUNDS_SLOTS: usize = MAX_TXS_IN_BATCH;

pub(crate) const OPERATOR_REFUNDS_OFFSET: usize = DEBUG_SLOTS_OFFSET
    + DEBUG_FIRST_SLOTS
    + PAYMASTER_CONTEXT_SLOTS
    + CURRENT_L2_TX_HASHES_SLOTS
    + NEW_FACTORY_DEPS_RESERVED_SLOTS;

pub(crate) const TX_OVERHEAD_OFFSET: usize = OPERATOR_REFUNDS_OFFSET + OPERATOR_REFUNDS_SLOTS;
pub(crate) const TX_OVERHEAD_SLOTS: usize = MAX_TXS_IN_BATCH;

pub(crate) const TX_TRUSTED_GAS_LIMIT_OFFSET: usize = TX_OVERHEAD_OFFSET + TX_OVERHEAD_SLOTS;
pub(crate) const TX_TRUSTED_GAS_LIMIT_SLOTS: usize = MAX_TXS_IN_BATCH;

pub(crate) const COMPRESSED_BYTECODES_SLOTS: usize = 196608;

pub(crate) const PRIORITY_TXS_L1_DATA_OFFSET: usize =
    COMPRESSED_BYTECODES_OFFSET + COMPRESSED_BYTECODES_SLOTS;
pub(crate) const PRIORITY_TXS_L1_DATA_SLOTS: usize = 2;

pub const OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_OFFSET: usize =
    PRIORITY_TXS_L1_DATA_OFFSET + PRIORITY_TXS_L1_DATA_SLOTS;

/// One of "worst case" scenarios for the number of state diffs in a batch is when 780kb of pubdata is spent
/// on repeated writes, that are all zeroed out. In this case, the number of diffs is 780kb / 5 = 156k. This means that they will have
/// accoomdate 42432000 bytes of calldata for the uncompressed state diffs. Adding 780kb on top leaves us with
/// roughly 43212000 bytes needed for calldata.
/// 1350375 slots are needed to accommodate this amount of data. We round up to 1360000 slots just in case.
///
/// In theory though much more calldata could be used (if for instance 1 byte is used for enum index). It is the responsibility of the
/// operator to ensure that it can form the correct calldata for the L1Messenger.
pub(crate) const OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS: usize = 1360000;

pub(crate) const BOOTLOADER_TX_DESCRIPTION_OFFSET: usize =
    OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_OFFSET + OPERATOR_PROVIDED_L1_MESSENGER_PUBDATA_SLOTS;

/// The size of the bootloader memory dedicated to the encodings of transactions
pub(crate) const BOOTLOADER_TX_ENCODING_SPACE: u32 =
    (USED_BOOTLOADER_MEMORY_WORDS - TX_DESCRIPTION_OFFSET - MAX_TXS_IN_BATCH) as u32;

// Size of the bootloader tx description in words
pub(crate) const BOOTLOADER_TX_DESCRIPTION_SIZE: usize = 2;

/// The actual descriptions of transactions should start after the minor descriptions and a MAX_POSTOP_SLOTS
/// free slots to allow postOp encoding.
pub(crate) const TX_DESCRIPTION_OFFSET: usize = BOOTLOADER_TX_DESCRIPTION_OFFSET
    + BOOTLOADER_TX_DESCRIPTION_SIZE * MAX_TXS_IN_BATCH
    + MAX_POSTOP_SLOTS;

pub(crate) const TX_GAS_LIMIT_OFFSET: usize = 4;

const INITIAL_BASE_PAGE: u32 = 8;
pub const BOOTLOADER_HEAP_PAGE: u32 = heap_page_from_base(MemoryPage(INITIAL_BASE_PAGE)).0;

/// VM Hooks are used for communication between bootloader and tracers.
/// The 'type' / 'opcode' is put into VM_HOOK_POSITION slot,
/// and VM_HOOKS_PARAMS_COUNT parameters (each 32 bytes) are put in the slots before.
/// So the layout looks like this:
/// `[param 0][param 1][vmhook opcode]`
pub const VM_HOOK_POSITION: u32 = RESULT_SUCCESS_FIRST_SLOT - 1;
pub const VM_HOOK_PARAMS_COUNT: u32 = 3;
pub const VM_HOOK_PARAMS_START_POSITION: u32 = VM_HOOK_POSITION - VM_HOOK_PARAMS_COUNT;

/// Arbitrary space in memory closer to the end of the page
pub const RESULT_SUCCESS_FIRST_SLOT: u32 =
    ((USED_BOOTLOADER_MEMORY_BYTES as u32) - (MAX_TXS_IN_BATCH as u32) * 32) / 32;

/// How many gas bootloader is allowed to spend within one block.
/// Note that this value doesn't correspond to the gas limit of any particular transaction
/// (except for the fact that, of course, gas limit for each transaction should be <= `BLOCK_GAS_LIMIT`).
pub const BLOCK_GAS_LIMIT: u32 =
    zk_evm_1_5_0::zkevm_opcode_defs::system_params::VM_INITIAL_FRAME_ERGS;

/// How many gas is allowed to spend on a single transaction in eth_call method
pub const ETH_CALL_GAS_LIMIT: u32 = MAX_L2_TX_GAS_LIMIT as u32;

/// ID of the transaction from L1
pub const L1_TX_TYPE: u8 = 255;

pub(crate) const TX_OPERATOR_L2_BLOCK_INFO_OFFSET: usize =
    TX_TRUSTED_GAS_LIMIT_OFFSET + TX_TRUSTED_GAS_LIMIT_SLOTS;

pub(crate) const TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO: usize = 4;
pub(crate) const TX_OPERATOR_L2_BLOCK_INFO_SLOTS: usize =
    (MAX_TXS_IN_BATCH + 1) * TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO;

pub(crate) const COMPRESSED_BYTECODES_OFFSET: usize =
    TX_OPERATOR_L2_BLOCK_INFO_OFFSET + TX_OPERATOR_L2_BLOCK_INFO_SLOTS;

/// The maximal gas limit that gets passed into an L1->L2 transaction.
///
/// It is equal to the `MAX_GAS_PER_TRANSACTION` in the bootloader.
/// We need to limit the number of gas that can be passed into the L1->L2 transaction due to the fact
/// that unlike L2 transactions they can not be rejected by the operator and must be executed. In turn,
/// this means that if a transaction spends more than `MAX_GAS_PER_TRANSACTION`, it use up all the limited resources of a batch.
///
/// It the gas limit cap on Mailbox for a priority transactions should generally be low enough to never cross that boundary, since
/// artificially limiting the gas price is bad UX. However, during the transition between the pre-1.4.1 fee model and the 1.4.1 one,
/// we need to process such transactions somehow.
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

const ZK_SYNC_BYTES_PER_BLOB: usize = BLOB_CHUNK_SIZE * ELEMENTS_PER_4844_BLOCK;
pub const MAX_BLOBS_PER_BATCH: usize = 6;
pub const MAX_VM_PUBDATA_PER_BATCH: usize = MAX_BLOBS_PER_BATCH * ZK_SYNC_BYTES_PER_BLOB;
