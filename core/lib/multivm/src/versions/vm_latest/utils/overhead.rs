use crate::vm_latest::constants::{TX_MEMORY_OVERHEAD_GAS, TX_SLOT_OVERHEAD_GAS};

pub(crate) fn derive_overhead(encoded_len: usize) -> u32 {
    TX_SLOT_OVERHEAD_GAS.max(TX_MEMORY_OVERHEAD_GAS * (encoded_len as u32))
}

pub(crate) fn get_amortized_overhead(encoded_len: usize) -> u32 {
    derive_overhead(encoded_len)
}
