pub fn derive_overhead(encoded_len: usize) -> u32 {
    // todo: move into constants
    let slot_overhead_gas = 10000;

    // todo: move into constants
    let memory_overhead_gas = 10;

    vec![
        slot_overhead_gas,
        memory_overhead_gas * (encoded_len as u32),
    ]
    .into_iter()
    .max()
    .unwrap()
}

pub fn get_amortized_overhead(encoded_len: usize) -> u32 {
    derive_overhead(encoded_len)
}
