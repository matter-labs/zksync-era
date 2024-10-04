use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use zksync_contracts::{read_zbin_bytecode, BaseSystemContracts, SystemContractCode};
use zksync_types::{
    utils::storage_key_for_standard_token_balance, AccountTreeId, Address, StorageKey, H160, H256,
    U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, h256_to_u256, u256_to_h256};
use zksync_vm2::interface::{HeapId, StateInterface};

use crate::interface::storage::ReadStorage;

pub(crate) static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

pub(crate) fn verify_required_memory(
    state: &impl StateInterface,
    required_values: Vec<(U256, HeapId, u32)>,
) {
    for (required_value, memory_page, cell) in required_values {
        let current_value = state.read_heap_u256(memory_page, cell * 32);
        assert_eq!(current_value, required_value);
    }
}

pub(crate) fn verify_required_storage(
    required_values: &[(H256, StorageKey)],
    main_storage: &mut impl ReadStorage,
    storage_changes: &BTreeMap<(H160, U256), U256>,
) {
    for &(required_value, key) in required_values {
        let current_value = storage_changes
            .get(&(*key.account().address(), h256_to_u256(*key.key())))
            .copied()
            .unwrap_or_else(|| h256_to_u256(main_storage.read_value(&key)));

        assert_eq!(
            u256_to_h256(current_value),
            required_value,
            "Invalid value at key {key:?}"
        );
    }
}
pub(crate) fn get_balance(
    token_id: AccountTreeId,
    account: &Address,
    main_storage: &mut impl ReadStorage,
    storage_changes: &BTreeMap<(H160, U256), U256>,
) -> U256 {
    let key = storage_key_for_standard_token_balance(token_id, account);

    storage_changes
        .get(&(*key.account().address(), h256_to_u256(*key.key())))
        .copied()
        .unwrap_or_else(|| h256_to_u256(main_storage.read_value(&key)))
}

pub(crate) fn get_bootloader(test: &str) -> SystemContractCode {
    let bootloader_code = read_zbin_bytecode(format!(
        "contracts/system-contracts/bootloader/tests/artifacts/{}.yul.zbin",
        test
    ));

    let bootloader_hash = hash_bytecode(&bootloader_code);
    SystemContractCode {
        code: bytes_to_be_words(bootloader_code),
        hash: bootloader_hash,
    }
}
