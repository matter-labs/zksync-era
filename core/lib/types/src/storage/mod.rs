use core::fmt::Debug;

use blake2::{Blake2s256, Digest};
pub use log::*;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::keccak256_concat, L2ChainId};
pub use zksync_system_constants::*;

use crate::{address_to_h256, AccountTreeId, Address, H160, H256, U256};

pub mod log;
pub mod witness_block_state;
pub mod writes;

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
pub fn get_address_mapping_key(address: &Address, position: H256) -> H256 {
    let padded_address = address_to_h256(address);
    keccak256_concat(padded_address, position)
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

/// Gets a slot holding EVM-compatible bytecode hash (i.e., `keccak256` of the non-padded EVM bytecode).
pub fn get_evm_code_hash_key(versioned_bytecode_hash: H256) -> StorageKey {
    StorageKey::new(
        AccountTreeId::new(EVM_HASHES_STORAGE_ADDRESS),
        versioned_bytecode_hash,
    )
}

pub fn get_known_code_key(hash: &H256) -> StorageKey {
    let known_codes_storage = AccountTreeId::new(KNOWN_CODES_STORAGE_ADDRESS);
    StorageKey::new(known_codes_storage, *hash)
}

pub fn get_system_context_key(key: H256) -> StorageKey {
    let system_context = AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS);
    StorageKey::new(system_context, key)
}

pub fn get_message_root_log_key(key: H256) -> StorageKey {
    let message_root = AccountTreeId::new(L2_MESSAGE_ROOT_ADDRESS);
    StorageKey::new(message_root, key)
}

pub fn get_deployer_key(key: H256) -> StorageKey {
    let deployer_contract = AccountTreeId::new(CONTRACT_DEPLOYER_ADDRESS);
    StorageKey::new(deployer_contract, key)
}

pub fn get_is_account_key(account: &Address) -> StorageKey {
    let deployer = AccountTreeId::new(CONTRACT_DEPLOYER_ADDRESS);

    // The `is_account` is stored in a mapping inside the deployer system contract
    let key = get_address_mapping_key(account, H256::zero());

    StorageKey::new(deployer, key)
}

pub fn get_immutable_simulator_key(account: &Address, immutable_key: H256) -> StorageKey {
    // All the immutable variables are stored as inside the ImmutableSimulator system contracts
    // in a mapping of the following form:
    // mapping(uint256 contractAddress => mapping(uint256 index => bytes32 value)) internal immutableDataStorage;

    let immutable_simulator = AccountTreeId::new(IMMUTABLE_SIMULATOR_STORAGE_ADDRESS);

    let padded_address = address_to_h256(account);

    let inner_mapping_slot = keccak256_concat(padded_address, IMMUTABLE_SIMULATOR_MAPPING_SLOT);
    let key = keccak256_concat(immutable_key, inner_mapping_slot);

    StorageKey::new(immutable_simulator, key)
}

pub type StorageValue = H256;

pub fn get_system_context_init_logs(chain_id: L2ChainId) -> Vec<StorageLog> {
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
