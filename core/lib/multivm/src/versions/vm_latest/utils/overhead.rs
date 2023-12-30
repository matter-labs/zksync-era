use crate::vm_latest::constants::{TX_MEMORY_OVERHEAD_GAS, TX_SLOT_OVERHEAD_GAS};

pub fn derive_overhead(encoded_len: usize) -> u32 {
    vec![
        TX_SLOT_OVERHEAD_GAS,
        TX_MEMORY_OVERHEAD_GAS * (encoded_len as u32),
    ]
    .into_iter()
    .max()
    .unwrap()
}

pub fn get_amortized_overhead(encoded_len: usize) -> u32 {
    derive_overhead(encoded_len)
}
