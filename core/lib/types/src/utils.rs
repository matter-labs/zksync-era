use crate::system_contracts::DEPLOYMENT_NONCE_INCREMENT;
use crate::L2_ETH_TOKEN_ADDRESS;
use crate::{web3::signing::keccak256, AccountTreeId, StorageKey, U256};

use zksync_basic_types::{Address, H256};

use zksync_utils::{address_to_h256, u256_to_h256};

/// Transforms the *full* account nonce into an *account* nonce.
/// Full nonce is a composite one: it includes both account nonce (number of transactions
/// initiated by the account) and deployer nonce (number of smart contracts deployed by the
/// account).
/// For most public things, we need the account nonce.
pub fn decompose_full_nonce(full_nonce: U256) -> (U256, U256) {
    (
        full_nonce % DEPLOYMENT_NONCE_INCREMENT,
        full_nonce / DEPLOYMENT_NONCE_INCREMENT,
    )
}

/// Converts tx nonce + deploy nonce into a full nonce.
pub fn nonces_to_full_nonce(tx_nonce: U256, deploy_nonce: U256) -> U256 {
    DEPLOYMENT_NONCE_INCREMENT * deploy_nonce + tx_nonce
}

fn key_for_eth_balance(address: &Address) -> H256 {
    let address_h256 = address_to_h256(address);

    let bytes = [address_h256.as_bytes(), &[0; 32]].concat();
    keccak256(&bytes).into()
}

/// Create a `key` part of `StorageKey` to access the balance from ERC20 contract balances
fn key_for_erc20_balance(address: &Address) -> H256 {
    let address_h256 = address_to_h256(address);

    // 20 bytes address first gets aligned to 32 bytes with index of `balanceOf` storage slot
    // of default ERC20 contract and to then to 64 bytes.
    let slot_index = H256::from_low_u64_be(51);
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(address_h256.as_bytes());
    bytes[32..].copy_from_slice(slot_index.as_bytes());
    H256(keccak256(&bytes))
}

/// Create a storage key to access the balance from supported token contract balances
pub fn storage_key_for_standard_token_balance(
    token_contract: AccountTreeId,
    address: &Address,
) -> StorageKey {
    // We have different implementation of the standard erc20 contract and native
    // eth contract. The key for the balance is different for each.
    let key = if token_contract.address() == &L2_ETH_TOKEN_ADDRESS {
        key_for_eth_balance(address)
    } else {
        key_for_erc20_balance(address)
    };

    StorageKey::new(token_contract, key)
}

pub fn storage_key_for_eth_balance(address: &Address) -> StorageKey {
    storage_key_for_standard_token_balance(AccountTreeId::new(L2_ETH_TOKEN_ADDRESS), address)
}

/// Pre-calculated the address of the to-be-deployed contract (via CREATE, not CREATE2).
pub fn deployed_address_create(sender: Address, deploy_nonce: U256) -> Address {
    let prefix_bytes = keccak256("zksyncCreate".as_bytes());
    let address_bytes = address_to_h256(&sender);
    let nonce_bytes = u256_to_h256(deploy_nonce);

    let mut bytes = [0u8; 96];
    bytes[..32].copy_from_slice(&prefix_bytes);
    bytes[32..64].copy_from_slice(address_bytes.as_bytes());
    bytes[64..].copy_from_slice(nonce_bytes.as_bytes());

    Address::from_slice(&keccak256(&bytes)[12..])
}

#[cfg(test)]
mod tests {
    use crate::{
        utils::storage_key_for_standard_token_balance, AccountTreeId, Address, StorageKey, H256,
    };
    use std::str::FromStr;

    #[test]
    fn test_storage_key_for_eth_token() {
        let contract = AccountTreeId::new(Address::zero());
        let addresses = [
            "0x1dfe8ea5e8de74634db78d9f8d41a1c832ab91e8",
            "0xde03a0b5963f75f1c8485b355ff6d30f3093bde7",
            "0x2c9fc71c164f7332f368da477256e1b049575979",
        ];
        let hashes = [
            "0xd8f16e1d7fe824994134861c968a8f276930db7daf6ba4dd083567259d3ff857",
            "0x4e08bf0f8822508eed9a1fb7d98cf6067ab156c74e9ebdda0924bef229d71995",
            "0xb6ef92f5b364b6e13f237aef1213b68f53f91ac35dcea0ad60e103b5245fd85c",
        ];
        for (address, hash) in addresses.iter().zip(hashes.iter()) {
            let addr = Address::from_str(address).unwrap();
            let user_key = H256::from_str(hash).unwrap();
            let expected_storage_key = StorageKey::new(contract, user_key);
            let calculated_storage_key = storage_key_for_standard_token_balance(contract, &addr);
            assert_eq!(expected_storage_key, calculated_storage_key);
        }
    }
}
