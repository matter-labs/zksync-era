use core::fmt::Debug;
use std::str::FromStr;

use blake2::{Blake2s256, Digest};
use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::keccak256, L2ChainId};

use crate::{AccountTreeId, Address, H160, H256, U256};

pub mod log;
pub mod writes;

pub use log::*;
pub use zksync_system_constants::*;
use zksync_utils::address_to_h256;

/// Typed fully qualified key of the storage slot in global state tree.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[derive(Serialize, Deserialize)]
pub struct StorageKey {
    account: AccountTreeId,
    key: H256,
}

impl StorageKey {
    pub fn new(account: AccountTreeId, key: H256) -> Self {
        Self { account, key }
    }

    pub fn account(&self) -> &AccountTreeId {
        &self.account
    }

    pub fn key(&self) -> &H256 {
        &self.key
    }

    pub fn address(&self) -> &Address {
        self.account.address()
    }

    pub fn raw_hashed_key(address: &H160, key: &H256) -> [u8; 32] {
        let mut bytes = [0u8; 64];
        bytes[12..32].copy_from_slice(&address.0);
        U256::from(key.to_fixed_bytes()).to_big_endian(&mut bytes[32..64]);

        Blake2s256::digest(bytes).into()
    }

    pub fn hashed_key(&self) -> H256 {
        Self::raw_hashed_key(self.address(), self.key()).into()
    }

    pub fn hashed_key_u256(&self) -> U256 {
        U256::from_little_endian(&Self::raw_hashed_key(self.address(), self.key()))
    }
}

// Returns the storage key where the value for mapping(address => x)
// at position `position` is stored.
fn get_address_mapping_key(address: &Address, position: H256) -> H256 {
    let padded_address = address_to_h256(address);
    H256(keccak256(
        &[padded_address.as_bytes(), position.as_bytes()].concat(),
    ))
}

pub fn get_immutable_key(address: &Address, index: H256) -> H256 {
    let padded_address = address_to_h256(address);

    // keccak256(uint256(9) . keccak256(uint256(4) . uint256(1)))

    let address_position =
        keccak256(&[padded_address.as_bytes(), H256::zero().as_bytes()].concat());

    H256(keccak256(&[index.as_bytes(), &address_position].concat()))
}

pub fn get_nonce_key(account: &Address) -> StorageKey {
    let nonce_manager = AccountTreeId::new(NONCE_HOLDER_ADDRESS);

    // The `minNonce` (used as nonce for EOAs) is stored in a mapping inside the `NONCE_HOLDER` system contract
    let key = get_address_mapping_key(account, H256::zero());

    StorageKey::new(nonce_manager, key)
}

pub fn get_code_key(account: &Address) -> StorageKey {
    let account_code_storage = AccountTreeId::new(ACCOUNT_CODE_STORAGE_ADDRESS);
    StorageKey::new(account_code_storage, address_to_h256(account))
}

pub fn get_known_code_key(hash: &H256) -> StorageKey {
    let known_codes_storage = AccountTreeId::new(KNOWN_CODES_STORAGE_ADDRESS);
    StorageKey::new(known_codes_storage, *hash)
}

pub fn get_system_context_key(key: H256) -> StorageKey {
    let system_context = AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS);
    StorageKey::new(system_context, key)
}

fn get_message_root_log_key(key: H256) -> StorageKey {
    let message_root = AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS);
    StorageKey::new(message_root, key)
}

fn get_immutable_simulator_log_key(key: H256) -> StorageKey {
    let immutable_simulator = AccountTreeId::new(IMMUTABLE_SIMULATOR_STORAGE_ADDRESS);
    StorageKey::new(immutable_simulator, key)
}

pub fn get_is_account_key(account: &Address) -> StorageKey {
    let deployer = AccountTreeId::new(CONTRACT_DEPLOYER_ADDRESS);

    // The `is_account` is stored in a mapping inside the deployer system contract
    let key = get_address_mapping_key(account, H256::zero());

    StorageKey::new(deployer, key)
}

pub type StorageValue = H256;

fn get_system_context_init_logs(chain_id: L2ChainId) -> Vec<StorageLog> {
    vec![
        StorageLog::new_write_log(
            get_system_context_key(SYSTEM_CONTEXT_CHAIN_ID_POSITION),
            H256::from_low_u64_be(chain_id.as_u64()),
        ),
        StorageLog::new_write_log(
            get_system_context_key(SYSTEM_CONTEXT_BLOCK_GAS_LIMIT_POSITION),
            SYSTEM_CONTEXT_BLOCK_GAS_LIMIT,
        ),
        StorageLog::new_write_log(
            get_system_context_key(SYSTEM_CONTEXT_COINBASE_POSITION),
            address_to_h256(&BOOTLOADER_ADDRESS),
        ),
        StorageLog::new_write_log(
            get_system_context_key(SYSTEM_CONTEXT_DIFFICULTY_POSITION),
            SYSTEM_CONTEXT_DIFFICULTY,
        ),
    ]
}

/// The slots that are initialized in the message root storage.
///
/// Typically all the contracts with complex initialization logic are initialized in the system
/// via the genesis upgrade. However, the `L2_MESSAGE_ROOT` contract must be initialized for every batch
/// test in order for L1Messenger to work.
/// In order to simplify testing, we always initialize it via hardcoding the slots in the genesis.
///
/// The constants below might seem magical. For now, our genesis only supports genesis from the latest version of the VM
/// and so for now the correctness of those values is tested in a unit tests within the multivm crate.
pub fn get_l2_message_root_init_logs() -> Vec<StorageLog> {
    let slots_values = vec![
        (
            "8e94fed44239eb2314ab7a406345e6c5a8f0ccedf3b600de3d004e672c33abf4",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000007",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "a66cc928b5edb82af9bd49922954155ab7b0942694bea4ce44661d9a8736c688",
            "46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21",
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000006",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "f652222313e28459528d920b65115c16c04f3efc82aaedc97be59f3f377c0d3f",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ),
        (
            "b868bdfa8727775661e4ccf117824a175a33f8703d728c04488fbfffcafda9f9",
            "46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21",
        ),
    ];
    let immutable_simulator_slot = StorageLog::new_write_log(
        get_immutable_simulator_log_key(
            H256::from_str("cb5ca2f778293159761b941dc7b8f7fd374e3632c39b35a0fd4b1aa20ed4a091")
                .unwrap(),
        ),
        H256::from_str("0000000000000000000000000000000000000000000000000000000000010002").unwrap(),
    );

    slots_values
        .into_iter()
        .map(|(k, v)| {
            let key = H256::from_str(k).unwrap();
            let value = H256::from_str(v).unwrap();

            StorageLog::new_write_log(get_message_root_log_key(key), value)
        })
        .chain(std::iter::once(immutable_simulator_slot))
        .collect()
}

pub fn get_system_contracts_init_logs(chain_id: L2ChainId) -> Vec<StorageLog> {
    let system_context_init_logs = get_system_context_init_logs(chain_id);
    let l2_message_root_init_logs = get_l2_message_root_init_logs();

    system_context_init_logs
        .into_iter()
        .chain(l2_message_root_init_logs)
        .collect()
}
