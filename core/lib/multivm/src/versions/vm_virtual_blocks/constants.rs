use zk_evm_1_3_3::aux_structures::MemoryPage;
pub use zk_evm_1_3_3::zkevm_opcode_defs::system_params::{
    ERGS_PER_CIRCUIT, INITIAL_STORAGE_WRITE_PUBDATA_BYTES, MAX_PUBDATA_PER_BLOCK,
};
use zksync_system_constants::{L1_GAS_PER_PUBDATA_BYTE, MAX_L2_TX_GAS_LIMIT};

use crate::vm_virtual_blocks::old_vm::utils::heap_page_from_base;

/// The size of the bootloader memory in bytes which is used by the protocol.
/// While the maximal possible size is a lot higher, we restrict ourselves to a certain limit to reduce
/// the requirements on RAM.
pub(crate) const USED_BOOTLOADER_MEMORY_BYTES: usize = 1 << 24;
pub(crate) const USED_BOOTLOADER_MEMORY_WORDS: usize = USED_BOOTLOADER_MEMORY_BYTES / 32;

// This the number of pubdata such that it should be always possible to publish
// from a single transaction. Note, that these pubdata bytes include only bytes that are
// to be published inside the body of transaction (i.e. excluding of factory deps).
pub(crate) const GUARANTEED_PUBDATA_PER_L1_BATCH: u64 = 4000;

// The users should always be able to provide `MAX_GAS_PER_PUBDATA_BYTE` gas per pubdata in their
// transactions so that they are able to send at least `GUARANTEED_PUBDATA_PER_L1_BATCH` bytes per
// transaction.
pub(crate) const MAX_GAS_PER_PUBDATA_BYTE: u64 =
    MAX_L2_TX_GAS_LIMIT / GUARANTEED_PUBDATA_PER_L1_BATCH;

// The maximal number of transactions in a single batch
pub(crate) const MAX_TXS_IN_BLOCK: usize = 1024;

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

pub(crate) const MAX_NEW_FACTORY_DEPS: usize = 32;

/// Slots used to store the calldata for the KnownCodesStorage to mark new factory
/// dependencies as known ones. Besides the slots for the new factory dependencies themselves
/// another 4 slots are needed for: selector, marker of whether the user should pay for the pubdata,
/// the offset for the encoding of the array as well as the length of the array.
const NEW_FACTORY_DEPS_RESERVED_SLOTS: usize = MAX_NEW_FACTORY_DEPS + 4;

/// The operator can provide for each transaction the proposed minimal refund
pub(crate) const OPERATOR_REFUNDS_SLOTS: usize = MAX_TXS_IN_BLOCK;

pub(crate) const OPERATOR_REFUNDS_OFFSET: usize = DEBUG_SLOTS_OFFSET
    + DEBUG_FIRST_SLOTS
    + PAYMASTER_CONTEXT_SLOTS
    + CURRENT_L2_TX_HASHES_SLOTS
    + NEW_FACTORY_DEPS_RESERVED_SLOTS;

pub(crate) const TX_OVERHEAD_OFFSET: usize = OPERATOR_REFUNDS_OFFSET + OPERATOR_REFUNDS_SLOTS;
pub(crate) const TX_OVERHEAD_SLOTS: usize = MAX_TXS_IN_BLOCK;

pub(crate) const TX_TRUSTED_GAS_LIMIT_OFFSET: usize = TX_OVERHEAD_OFFSET + TX_OVERHEAD_SLOTS;
pub(crate) const TX_TRUSTED_GAS_LIMIT_SLOTS: usize = MAX_TXS_IN_BLOCK;

pub(crate) const COMPRESSED_BYTECODES_SLOTS: usize = 32768;

pub(crate) const BOOTLOADER_TX_DESCRIPTION_OFFSET: usize =
    COMPRESSED_BYTECODES_OFFSET + COMPRESSED_BYTECODES_SLOTS;

/// The size of the bootloader memory dedicated to the encodings of transactions
pub(crate) const BOOTLOADER_TX_ENCODING_SPACE: u32 =
    (USED_BOOTLOADER_MEMORY_WORDS - TX_DESCRIPTION_OFFSET - MAX_TXS_IN_BLOCK) as u32;

// Size of the bootloader tx description in words
pub(crate) const BOOTLOADER_TX_DESCRIPTION_SIZE: usize = 2;

/// The actual descriptions of transactions should start after the minor descriptions and a MAX_POSTOP_SLOTS
/// free slots to allow postOp encoding.
pub(crate) const TX_DESCRIPTION_OFFSET: usize = BOOTLOADER_TX_DESCRIPTION_OFFSET
    + BOOTLOADER_TX_DESCRIPTION_SIZE * MAX_TXS_IN_BLOCK
    + MAX_POSTOP_SLOTS;

pub(crate) const TX_GAS_LIMIT_OFFSET: usize = 4;

const INITIAL_BASE_PAGE: u32 = 8;
pub const BOOTLOADER_HEAP_PAGE: u32 = heap_page_from_base(MemoryPage(INITIAL_BASE_PAGE)).0;
pub(crate) const BLOCK_OVERHEAD_GAS: u32 = 1200000;
pub(crate) const BLOCK_OVERHEAD_L1_GAS: u32 = 1000000;
pub const BLOCK_OVERHEAD_PUBDATA: u32 = BLOCK_OVERHEAD_L1_GAS / L1_GAS_PER_PUBDATA_BYTE;

/// VM Hooks are used for communication between bootloader and tracers.
///
/// The 'type' / 'opcode' is put into VM_HOOK_POSITION slot,
/// and VM_HOOKS_PARAMS_COUNT parameters (each 32 bytes) are put in the slots before.
/// So the layout looks like this:
/// `[param 0][param 1][vmhook opcode]`
pub const VM_HOOK_POSITION: u32 = RESULT_SUCCESS_FIRST_SLOT - 1;
pub const VM_HOOK_PARAMS_COUNT: u32 = 2;
pub const VM_HOOK_PARAMS_START_POSITION: u32 = VM_HOOK_POSITION - VM_HOOK_PARAMS_COUNT;

pub(crate) const MAX_MEM_SIZE_BYTES: u32 = 16777216; // 2^24

/// Arbitrary space in memory closer to the end of the page
pub const RESULT_SUCCESS_FIRST_SLOT: u32 =
    (MAX_MEM_SIZE_BYTES - (MAX_TXS_IN_BLOCK as u32) * 32) / 32;

/// How many gas bootloader is allowed to spend within one block.
/// Note that this value doesn't correspond to the gas limit of any particular transaction
/// (except for the fact that, of course, gas limit for each transaction should be <= `BLOCK_GAS_LIMIT`).
pub(crate) const BLOCK_GAS_LIMIT: u32 =
    zk_evm_1_3_3::zkevm_opcode_defs::system_params::VM_INITIAL_FRAME_ERGS;

/// How many gas is allowed to spend on a single transaction in eth_call method
pub const ETH_CALL_GAS_LIMIT: u32 = MAX_L2_TX_GAS_LIMIT as u32;

/// ID of the transaction from L1
pub const L1_TX_TYPE: u8 = 255;

pub(crate) const TX_OPERATOR_L2_BLOCK_INFO_OFFSET: usize =
    TX_TRUSTED_GAS_LIMIT_OFFSET + TX_TRUSTED_GAS_LIMIT_SLOTS;

pub(crate) const TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO: usize = 4;
pub(crate) const TX_OPERATOR_L2_BLOCK_INFO_SLOTS: usize =
    (MAX_TXS_IN_BLOCK + 1) * TX_OPERATOR_SLOTS_PER_L2_BLOCK_INFO;

pub(crate) const COMPRESSED_BYTECODES_OFFSET: usize =
    TX_OPERATOR_L2_BLOCK_INFO_OFFSET + TX_OPERATOR_L2_BLOCK_INFO_SLOTS;
