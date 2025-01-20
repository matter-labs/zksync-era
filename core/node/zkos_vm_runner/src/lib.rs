use zk_ee::common_structs::derive_flat_storage_key;
use zk_ee::utils::Bytes32;
use zk_os_system_hooks::addresses_constants::ACCOUNT_PARTIAL_DATA_STORAGE_ADDRESS;
use zksync_types::{Address, address_to_h256, H256, h256_to_address};
use crate::zkos_conversions::{bytes32_to_h256, h256_to_bytes32};

pub mod zkos_conversions;

pub fn zkos_nonce_flat_key(address: Address) -> H256 {
    let nonce_holder = ACCOUNT_PARTIAL_DATA_STORAGE_ADDRESS;
    let key = h256_to_bytes32(address_to_h256(&address));
    bytes32_to_h256(derive_flat_storage_key(
        &nonce_holder,
        &key
    ))

}
