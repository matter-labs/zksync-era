use num::BigUint;
use once_cell::sync::Lazy;

pub const ZKPORTER_IS_AVAILABLE: bool = false;

/// Depth of the account tree.
pub const ROOT_TREE_DEPTH: usize = 256;
/// Cost of 1 byte of calldata in bytes.
// TODO (SMA-1609): Double check this value.
// TODO: possibly remove this value.
pub const GAS_PER_PUBDATA_BYTE: u32 = 16;

/// Maximum amount of bytes in one packed write storage slot.
/// Calculated as `(len(hash) + 1) + len(u256)`
// TODO (SMA-1609): Double check this value.
pub const MAX_BYTES_PER_PACKED_SLOT: u64 = 65;

/// Amount of gas required to publish one slot in pubdata.
pub static GAS_PER_SLOT: Lazy<BigUint> =
    Lazy::new(|| BigUint::from(MAX_BYTES_PER_PACKED_SLOT) * BigUint::from(GAS_PER_PUBDATA_BYTE));

pub const MAX_TXS_IN_BLOCK: usize = 1024;

pub const MAX_NEW_FACTORY_DEPS: usize = 32;

pub const PAD_MSG_BEFORE_HASH_BITS_LEN: usize = 736;

/// The size of the bootloader memory in bytes which is used by the protocol.
/// While the maximal possible size is a lot higher, we restric ourselves to a certain limit to reduce
/// the requirements on RAM.
pub const USED_BOOTLOADER_MEMORY_BYTES: usize = 1 << 24;
pub const USED_BOOTLOADER_MEMORY_WORDS: usize = USED_BOOTLOADER_MEMORY_BYTES / 32;
