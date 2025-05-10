use std::fmt;

use chrono::{DateTime, TimeZone, Utc};
use zksync_basic_types::{ethabi, Address, H256};
use zksync_system_constants::L2_NATIVE_TOKEN_VAULT_ADDRESS;

use crate::{
    address_to_h256, system_contracts::DEPLOYMENT_NONCE_INCREMENT, u256_to_h256, web3::keccak256,
    AccountTreeId, StorageKey, L2_BASE_TOKEN_ADDRESS, U256,
};

/// Displays a Unix timestamp (seconds since epoch) in human-readable form. Useful for logging.
pub fn display_timestamp(timestamp: u64) -> impl fmt::Display {
    enum DisplayedTimestamp {
        Parsed(DateTime<Utc>),
        Raw(u64),
    }

    impl fmt::Display for DisplayedTimestamp {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Parsed(timestamp) => fmt::Display::fmt(timestamp, formatter),
                Self::Raw(raw) => write!(formatter, "(raw: {raw})"),
            }
        }
    }

    let parsed = i64::try_from(timestamp).ok();
    let parsed = parsed.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
    parsed.map_or(
        DisplayedTimestamp::Raw(timestamp),
        DisplayedTimestamp::Parsed,
    )
}

/// Transforms the *full* account nonce into an *account* nonce.
///
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

pub fn key_for_eth_balance(address: &Address) -> H256 {
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
    // We have different implementation of the standard ERC20 contract and native
    // eth contract. The key for the balance is different for each.
    let key = if token_contract.address() == &L2_BASE_TOKEN_ADDRESS {
        key_for_eth_balance(address)
    } else {
        key_for_erc20_balance(address)
    };

    StorageKey::new(token_contract, key)
}

pub fn storage_key_for_eth_balance(address: &Address) -> StorageKey {
    storage_key_for_standard_token_balance(AccountTreeId::new(L2_BASE_TOKEN_ADDRESS), address)
}

/// Pre-calculates the address of the to-be-deployed EraVM contract (via CREATE, not CREATE2).
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

/// Pre-calculates the address of the EVM contract created using a deployment transaction.
pub fn deployed_address_evm_create(sender: Address, tx_nonce: U256) -> Address {
    let mut stream = rlp::RlpStream::new();
    stream
        .begin_unbounded_list()
        .append(&sender)
        .append(&tx_nonce)
        .finalize_unbounded_list();
    Address::from_slice(&keccak256(&stream.out())[12..])
}

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = ethabi::encode(&[
        ethabi::Token::Uint(l1_chain_id),
        ethabi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethabi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

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

    // Test vectors are taken from geth: https://github.com/ethereum/go-ethereum/blob/033de2a05bdbea87b4efc5156511afe42c38fd55/crypto/crypto_test.go#L133
    #[test]
    fn deployment_address_is_correctly_evaluated() {
        let sender: Address = "0x970e8128ab834e8eac17ab8e3812f010678cf791"
            .parse()
            .unwrap();
        let address0 = deployed_address_evm_create(sender, 0.into());
        assert_eq!(
            address0,
            "0x333c3310824b7c685133f2bedb2ca4b8b4df633d"
                .parse()
                .unwrap()
        );
        let address1 = deployed_address_evm_create(sender, 1.into());
        assert_eq!(
            address1,
            "0x8bda78331c916a08481428e4b07c96d3e916d165"
                .parse()
                .unwrap()
        );
        let address2 = deployed_address_evm_create(sender, 2.into());
        assert_eq!(
            address2,
            "0xc9ddedf451bc62ce88bf9292afb13df35b670699"
                .parse()
                .unwrap()
        );
    }
}
