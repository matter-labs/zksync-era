use blake2::{Blake2s256, Digest};
use zksync_basic_types::Address;
use zksync_types::U256;

pub fn derive_final_address_for_params(address: &Address, key: &U256) -> [u8; 32] {
    let mut buffer = [0u8; 64];
    buffer[12..32].copy_from_slice(&address.0);
    key.to_big_endian(&mut buffer[32..64]);

    let mut result = [0u8; 32];
    result.copy_from_slice(Blake2s256::digest(buffer).as_slice());

    result
}
