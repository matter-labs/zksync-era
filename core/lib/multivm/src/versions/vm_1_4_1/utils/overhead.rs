use crate::vm_1_4_1::constants::{TX_MEMORY_OVERHEAD_GAS, TX_SLOT_OVERHEAD_GAS};

/// In the past, the overhead for transaction depended also on the effective gas per pubdata limit for the transaction.
/// Currently, the approach is more similar to EVM, where only the calldata length and the transaction overhead are taken
/// into account by a constant formula.
pub(crate) fn derive_overhead(encoded_len: usize) -> u32 {
    TX_SLOT_OVERHEAD_GAS.max(TX_MEMORY_OVERHEAD_GAS * (encoded_len as u32))
}
