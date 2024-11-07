use zksync_basic_types::{Address, H256, U256};

fn ensure_chunkable(bytes: &[u8]) {
    assert!(
        bytes.len() % 32 == 0,
        "Bytes must be divisible by 32 to split into chunks"
    );
}

pub fn h256_to_u256(num: H256) -> U256 {
    U256::from_big_endian(num.as_bytes())
}

pub fn address_to_h256(address: &Address) -> H256 {
    let mut buffer = [0u8; 32];
    buffer[12..].copy_from_slice(address.as_bytes());
    H256(buffer)
}

pub fn address_to_u256(address: &Address) -> U256 {
    h256_to_u256(address_to_h256(address))
}

// FIXME: remove
pub fn bytes_to_chunks(bytes: &[u8]) -> Vec<[u8; 32]> {
    ensure_chunkable(bytes);
    bytes
        .chunks(32)
        .map(|el| {
            let mut chunk = [0u8; 32];
            chunk.copy_from_slice(el);
            chunk
        })
        .collect()
}

pub fn u256_to_h256(num: U256) -> H256 {
    let mut bytes = [0u8; 32];
    num.to_big_endian(&mut bytes);
    H256::from_slice(&bytes)
}

/// Converts `U256` value into the Address
pub fn u256_to_address(value: &U256) -> Address {
    let mut bytes = [0u8; 32];
    value.to_big_endian(&mut bytes);

    Address::from_slice(&bytes[12..])
}

/// Converts `H256` value into the Address
pub fn h256_to_address(value: &H256) -> Address {
    Address::from_slice(&value.as_bytes()[12..])
}
