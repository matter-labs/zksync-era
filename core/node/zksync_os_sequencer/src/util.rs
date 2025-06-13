use zk_ee::utils::Bytes32;
use zksync_types::{Address, address_to_h256, H256, h256_to_address};

pub fn address_to_bytes32(input: &Address) -> Bytes32 {
    let h256 = address_to_h256(input);
    let mut new = Bytes32::zero();
    new.as_u8_array_mut().copy_from_slice(h256.as_bytes());
    new
}

pub fn bytes32_to_address(input: &Bytes32) -> Address {
    let h256 = H256(input.as_u8_array());
    h256_to_address(&h256)
}
