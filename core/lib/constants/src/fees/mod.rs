mod intrinsic;

pub use intrinsic::*;

pub struct IntrinsicSystemGasConstants {
    // The overhead for each L2 transaction in computation (it is assumed that it is roughly independent of its structure)
    pub l2_tx_intrinsic_gas: u32,
    // The overhead for each L2 transaction in pubdata (it is assumed that it is roughly independent of its structure)
    pub l2_tx_intrinsic_pubdata: u32,
    // The number of gas the refund transfer requires
    pub l2_tx_gas_for_refund_transfer: u32,
    // The overhead for each L1 transaction in computation (it is assumed that it is roughly independent of its structure)
    pub l1_tx_intrinsic_gas: u32,
    // The overhead for each L1 transaction in pubdata (it is assumed that it is roughly independent of its structure)
    pub l1_tx_intrinsic_pubdata: u32,
    // The minimal price that each L1 transaction should cost to cover mandatory non-intrinsic parts.
    // For instance, the user should be always able to hash at least some minimal transaction.
    // (it is assumed that it is roughly independent of its structure)
    pub l1_tx_min_gas_base: u32,
    // The number of gas the transaction gains based on its length in words
    // (for each 544 bytes, the number is not a coincidence, since each new 136
    // bytes require a new keccak round and the length of the encoding increases by 32 bytes at a time)
    pub l1_tx_delta_544_encoding_bytes: u32,
    // The number of gas an L1->L2 transaction gains with each new factory dependency
    pub l1_tx_delta_factory_dep_gas: u32,
    // The number of pubdata an L1->L2 transaction requires with each new factory dependency
    pub l1_tx_delta_factory_dep_pubdata: u32,
    // The number of computational gas the bootloader requires
    pub bootloader_intrinsic_gas: u32,
    // The number of overhead pubdata the bootloader requires
    pub bootloader_intrinsic_pubdata: u32,
    // The number of memory available for transaction encoding
    pub bootloader_tx_memory_size_slots: u32,
}

/// The amount of gas we need to pay for each non-zero pubdata byte.
/// Note that it is bigger than 16 to account for potential overhead
pub const L1_GAS_PER_PUBDATA_BYTE: u32 = 17;

/// The amount of pubdata that is strictly guaranteed to be available for a block
pub const GUARANTEED_PUBDATA_IN_TX: u32 = 100000;

/// The amount of overhead that is paid when the bytecode is published onchain.
///
/// It comes from the 64 bytes of additional two words for the bytecode length and offset in the ABI-encoding
/// of the commitment. The other "36" bytes are mostly an approximation of the amount of gas it takes
/// to properly hash it and compare with the corresponding L2->L1 message.
pub const PUBLISH_BYTECODE_OVERHEAD: u32 = 100;

/// For processing big calldata contracts copy it and do some hashing. These two constants
/// represents the corresponding SL
pub const GATEWAY_CALLDATA_PROCESSING_ROLLUP_OVERHEAD_GAS: u32 = 200;
pub const L1_CALLDATA_PROCESSING_ROLLUP_OVERHEAD_GAS: u32 = 50;
